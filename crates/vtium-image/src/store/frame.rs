//! The frame's placement list (ADR-1100, D9).

use std::collections::BTreeMap;

use std::borrow::Cow;

use super::runs::{RUN_KEY_FIRST, Virtuals, origins, run_placement};
use super::{Animation, FramePixels, Located, Placement, Rows, Spot, Store};
use crate::image::{ImageKey, LineKey, Pixels, PlaceholderRun, PlacementKey};
use crate::rules::{Layer, rules};

/// A placement as a frame shows it: what the rules resolved, so the
/// compositor reads no tag and asks no rule, and the image's pixels,
/// shared.
#[derive(Clone, Debug, PartialEq)]
pub struct FramePlacement {
    pub key: PlacementKey,
    pub image: ImageKey,
    /// The view row of the box's top, negative when it is above the view.
    pub row: i64,
    pub col: u32,
    /// The box in cells.
    pub cols: u32,
    pub rows: u32,
    /// The picture's visible part: `width` by `height` pixels at `x`, `y`
    /// from the box's top left, in pixels of the cell it was made in.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// The image's pixels under the visible part: exact at 1:1, otherwise
    /// every source pixel any visible pixel shows.
    pub src_x: u32,
    pub src_y: u32,
    pub src_width: u32,
    pub src_height: u32,
    /// The whole placement's source rectangle in the image's pixels, its
    /// crop, before the view or a cut clipped it.
    pub crop_x: u32,
    pub crop_y: u32,
    pub crop_width: u32,
    pub crop_height: u32,
    /// The whole drawn picture, the crop at its scale: its top left at
    /// `offset_x`, `offset_y` from the box's top left, negative where a
    /// cut or a scroll left only its lower or right part in the box, and
    /// `drawn_width` by `drawn_height` pixels. A compositor that scales
    /// the whole picture once maps the visible part through these.
    pub offset_x: i64,
    pub offset_y: i64,
    pub drawn_width: u64,
    pub drawn_height: u64,
    /// The cell size the placement was made in, the unit of its pixels.
    pub cell_width: u32,
    pub cell_height: u32,
    pub z: i32,
    pub layer: Layer,
    /// The kind's printed name, from its rules.
    pub kind: &'static str,
    pub image_width: u32,
    pub image_height: u32,
    /// No pixel of the frame shown is below full alpha. For an animated
    /// image the store fills in whether every one of its frames is opaque
    /// (ADR-1102, D1).
    pub opaque: bool,
    /// The pixels of the frame shown, as `frame` and `pixel_revision` name
    /// them.
    pub pixels: Pixels,
    /// The store's [`revision`](Store::revision) when the placement or its
    /// image last changed.
    pub revision: u64,
    /// The number of the frame shown, 1 for the root frame and for a
    /// picture that is not animated (ADR-1102, D3). The store fills in the
    /// image's current frame, the one `a=a` with `c` made current; the
    /// binary's snapshot puts the frame its animation clock chose with
    /// [`show`](Self::show).
    pub frame: u32,
    /// That frame's pixel revision: the store's revision when a command last
    /// changed what the frame shows. The image's key, `frame` and
    /// `pixel_revision` together name the pixels, and a renderer keys what
    /// it keeps for a picture on all three.
    pub pixel_revision: u64,
    /// The image's animation, as the store records it from the commands,
    /// for the binary's animation clock; `None` for a picture no animation
    /// command named.
    pub animation: Option<Animation>,
}

impl FramePlacement {
    /// Whether this entry was made from a run of kitty Unicode-placeholder
    /// cells, rather than being one of the store's own placements with a
    /// key of its own.
    pub fn from_placeholder_run(&self) -> bool {
        self.key.get() >= RUN_KEY_FIRST
    }

    /// Shows `frame`, a frame of this entry's image from
    /// [`Store::frame_pixels`], in place of the one the entry holds: its
    /// pixels, its number and its pixel revision. Every placement of an
    /// image shows the same frame, so the snapshot puts the same frame on
    /// each.
    pub fn show(&mut self, frame: FramePixels) {
        self.pixels = frame.pixels;
        self.frame = frame.frame;
        self.pixel_revision = frame.revision;
    }
}

impl Store {
    /// The placements of the buffer in use with a row in the view, in draw
    /// order (D4): by layer, then z, then their place in the stack at that
    /// depth. `view.screen` is the view's lines top to bottom and
    /// `view.above` the lines above its top, newest first, as many as
    /// [`reach`](Self::reach); `cols` is the screen's width. A relative
    /// placement draws where its parent's origin and its offset put it,
    /// cut at the screen's left edge, and draws nothing while it follows a
    /// virtual placement (D13); a virtual placement draws nothing itself.
    /// A drawn frame moves no clock. A relative placement whose root lies
    /// below the view, and which starts above its root, draws only through
    /// [`frame_with_below`](Self::frame_with_below).
    pub fn frame(&self, view: Rows<'_>, cols: usize) -> Vec<FramePlacement> {
        self.frame_with_below(view, &[], cols)
    }

    /// The lines below the view's bottom a frame of the buffer in use must
    /// be given: the most rows any relative placement starts above the line
    /// its root is anchored on, so that a child whose root has scrolled out
    /// below a view moved back into the scrollback still draws.
    pub fn reach_below(&self) -> usize {
        self.buffers
            .get(self.active())
            .map_or(0, super::Buffer::reach_below)
    }

    /// [`frame`](Self::frame), given also `below`, the lines below the
    /// view's bottom, top to bottom, as many as
    /// [`reach_below`](Self::reach_below): a relative placement whose root
    /// is anchored on one of them draws where its box reaches the view.
    pub fn frame_with_below(
        &self,
        view: Rows<'_>,
        below: &[LineKey],
        cols: usize,
    ) -> Vec<FramePlacement> {
        self.frame_with_runs(view, below, &[], cols)
    }

    /// [`frame_with_below`](Self::frame_with_below), given also `runs`, the
    /// runs of kitty placeholder cells vtium-term read from the rows the
    /// view shows (ADR-1100, D13). Each run of a virtual placement of the
    /// buffer in use becomes an entry: the part of the placement's picture,
    /// fitted to its grid of cells, under the run's cells, anchored at the
    /// run's line and first column. A run whose image or virtual placement
    /// is not found, or whose line the view does not show, makes no entry,
    /// and the cells of a run outside the grid or the screen, or that the
    /// fitted picture does not reach, draw nothing. The entries take their
    /// virtual placement's place in the draw order, row by row, then column
    /// by column, and each has a key of its own among the frame's entries,
    /// from [`RUN_KEY_FIRST`](super::runs::RUN_KEY_FIRST) up. A relative
    /// placement whose parent is virtual draws from the origin its parent's
    /// runs give, and draws nothing when none of them is handed. The frame
    /// takes at most one run for each cell of the view.
    pub fn frame_with_runs(
        &self,
        view: Rows<'_>,
        below: &[LineKey],
        runs: &[PlaceholderRun],
        cols: usize,
    ) -> Vec<FramePlacement> {
        let Some(buf) = self.buffers.get(self.active()) else {
            return Vec::new();
        };
        if buf.placements.is_empty() {
            return Vec::new();
        }
        let order: BTreeMap<PlacementKey, usize> = buf
            .stacks
            .values()
            .flatten()
            .enumerate()
            .map(|(i, &k)| (k, i))
            .collect();
        let place = |k: &PlacementKey| order.get(k).copied().unwrap_or(usize::MAX);
        let screen = view.screen.iter().zip(0i64..);
        let above = view.above.iter().zip((1i64..).map(i64::wrapping_neg));
        let mut found: Vec<Found<'_>> = Vec::new();
        let mut line_rows: BTreeMap<LineKey, i64> = BTreeMap::new();
        for (line, row) in screen.chain(above) {
            if !buf.children.is_empty() {
                line_rows.insert(*line, row);
            }
            for &k in buf.by_line.get(line).into_iter().flatten() {
                let Some(p) = buf.placements.get(&k) else {
                    continue;
                };
                let in_cols = usize::try_from(p.col).is_ok_and(|c| c < cols);
                if in_cols && row.saturating_add(i64::from(p.rows)) > 0 {
                    found.push(Found::own(place(&k), row, k, p));
                }
            }
        }
        let height = i64::try_from(view.screen.len()).unwrap_or(i64::MAX);
        if !buf.children.is_empty() {
            for (line, row) in below.iter().zip(height..) {
                line_rows.insert(*line, row);
            }
        }
        // the runs the view's cells can hold, on the view's own rows
        let cap = view.screen.len().saturating_mul(cols);
        let runs = runs.get(..runs.len().min(cap)).unwrap_or_default();
        let virtuals = if runs.is_empty() {
            None
        } else {
            Some(Virtuals::of(buf))
        };
        let screen_rows: BTreeMap<LineKey, i64> = if runs.is_empty() {
            BTreeMap::new()
        } else {
            view.screen.iter().copied().zip(0i64..).collect()
        };
        let row_of = |line: LineKey| screen_rows.get(&line).copied();
        let origins = match &virtuals {
            Some(v) if !buf.children.is_empty() => origins(v, runs, row_of),
            _ => BTreeMap::new(),
        };
        let width = i64::try_from(cols).unwrap_or(i64::MAX);
        for &k in buf.children.values().flatten() {
            let Some(p) = buf.placements.get(&k) else {
                continue;
            };
            let (top, col, revision) = match buf.locate(k) {
                Some(Located::At {
                    line,
                    row,
                    col,
                    revision,
                    ..
                }) => match line_rows.get(&line) {
                    Some(r) => (r.saturating_add(row), col, revision),
                    None => continue,
                },
                Some(Located::Floating {
                    root,
                    row,
                    col,
                    revision,
                }) => match origins.get(&root) {
                    Some(&(top, left)) => {
                        (top.saturating_add(row), left.saturating_add(col), revision)
                    }
                    None => continue,
                },
                None => continue,
            };
            let shows = top.saturating_add(i64::from(p.rows)) > 0 && top < height && col < width;
            let Some(placed) = shows.then(|| at_column(p, col)).flatten() else {
                continue;
            };
            found.push(Found {
                order: place(&k),
                row: top,
                col: placed.col,
                key: k,
                placement: Cow::Owned(placed),
                revision,
            });
        }
        if let Some(virtuals) = &virtuals {
            for run in runs {
                let (Some(k), Some(row)) = (virtuals.find(run), row_of(run.line)) else {
                    continue;
                };
                let Some(placed) = buf
                    .placements
                    .get(&k)
                    .and_then(|p| run_placement(p, run, cols))
                else {
                    continue;
                };
                found.push(Found {
                    order: place(&k),
                    row,
                    col: placed.col,
                    key: k,
                    revision: placed.revision,
                    placement: Cow::Owned(placed),
                });
            }
        }
        found.sort_by_key(|f| (f.order, f.row, f.col, f.key));
        let mut next_run = RUN_KEY_FIRST;
        found
            .into_iter()
            .filter_map(|f| {
                let image = buf.images.get(&f.placement.image)?;
                // a run's entry takes a key of its own among the frame's
                let key = if f.placement.spot == Spot::Virtual {
                    let key = PlacementKey(next_run);
                    next_run = next_run.saturating_add(1);
                    key
                } else {
                    f.key
                };
                let shown = self.shown(f.placement.image, image);
                Some(entry(key, &f.placement, image, f.row, f.revision, shown))
            })
            .collect()
    }

    /// The frame an entry of `image`, whose key is `key`, shows: the image's
    /// current frame, as the commands set it, when that is a frame past the
    /// root; the binary's animation clock may put another in its place
    /// (ADR-1102, D3).
    fn shown(&self, key: ImageKey, image: &super::Image) -> Option<FramePixels> {
        let current = image.anim.as_deref()?.state().current;
        (current > 1)
            .then(|| self.frame_pixels(key, current))
            .flatten()
    }
}

/// A placement found for a frame: its place in the draw order, its view
/// row and column, its key, the placement as it draws and its revision.
struct Found<'a> {
    order: usize,
    row: i64,
    col: u32,
    key: PlacementKey,
    placement: Cow<'a, Placement>,
    revision: u64,
}

impl<'a> Found<'a> {
    fn own(order: usize, row: i64, key: PlacementKey, p: &'a Placement) -> Self {
        Self {
            order,
            row,
            col: p.col,
            key,
            placement: Cow::Borrowed(p),
            revision: p.revision,
        }
    }
}

/// Relative placement `p` with its box's left edge at column `col`: a part
/// left of the screen is cut off, as a cut piece's is, and `None` when no
/// part of its picture is left.
fn at_column(p: &Placement, col: i64) -> Option<Placement> {
    let left = col.min(0).unsigned_abs();
    let cut = u32::try_from(left).ok().filter(|&c| c < p.cols)?;
    let ox =
        p.ox.saturating_sub(i64::from(cut).saturating_mul(i64::from(p.cw)));
    let dw = i64::try_from(p.dw).unwrap_or(i64::MAX);
    if ox.saturating_add(dw) <= 0 {
        return None;
    }
    Some(Placement {
        col: u32::try_from(col.max(0)).unwrap_or(u32::MAX),
        cols: p.cols.saturating_sub(cut),
        ox,
        ..p.clone()
    })
}

/// `p`'s frame entry, its top at view row `row`, stamped with `revision`,
/// the latest of its own and its parents'.
fn entry(
    key: PlacementKey,
    p: &Placement,
    image: &super::Image,
    row: i64,
    revision: u64,
    shown: Option<FramePixels>,
) -> FramePlacement {
    let dw = i64::try_from(p.dw).unwrap_or(i64::MAX).max(1);
    let dh = i64::try_from(p.dh).unwrap_or(i64::MAX).max(1);
    let bw = i64::from(p.cols).saturating_mul(i64::from(p.cw));
    let bh = i64::from(p.rows).saturating_mul(i64::from(p.ch));
    let x0 = p.ox.max(0);
    let y0 = p.oy.max(0);
    let x1 = p.ox.saturating_add(dw).min(bw);
    let y1 = p.oy.saturating_add(dh).min(bh);
    let (sx, sy) = (i64::from(p.sx), i64::from(p.sy));
    let (sw, sh) = (i64::from(p.sw), i64::from(p.sh));
    let first = |o: i64, s: i64, ss: i64, d: i64, v: i64| {
        s.saturating_add(
            v.saturating_sub(o)
                .saturating_mul(ss)
                .checked_div(d)
                .unwrap_or(0),
        )
    };
    let end = |o: i64, s: i64, ss: i64, d: i64, v: i64| {
        s.saturating_add(
            v.saturating_sub(o)
                .saturating_mul(ss)
                .saturating_add(d.saturating_sub(1))
                .checked_div(d)
                .unwrap_or(0),
        )
    };
    let src_x = first(p.ox, sx, sw, dw, x0);
    let src_y = first(p.oy, sy, sh, dh, y0);
    let src_w = end(p.ox, sx, sw, dw, x1).saturating_sub(src_x);
    let src_h = end(p.oy, sy, sh, dh, y1).saturating_sub(src_y);
    let u = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
    FramePlacement {
        key,
        image: p.image,
        row,
        col: p.col,
        cols: p.cols,
        rows: p.rows,
        x: u(x0),
        y: u(y0),
        width: u(x1.saturating_sub(x0)),
        height: u(y1.saturating_sub(y0)),
        src_x: u(src_x),
        src_y: u(src_y),
        src_width: u(src_w),
        src_height: u(src_h),
        crop_x: p.sx,
        crop_y: p.sy,
        crop_width: p.sw,
        crop_height: p.sh,
        offset_x: p.ox,
        offset_y: p.oy,
        drawn_width: p.dw,
        drawn_height: p.dh,
        cell_width: p.cw,
        cell_height: p.ch,
        z: p.z,
        layer: p.layer,
        kind: rules(p.protocol.0).name,
        image_width: image.width,
        image_height: image.height,
        opaque: image.opaque && image.anim.as_deref().is_none_or(super::Anim::opaque),
        pixels: shown
            .as_ref()
            .map_or_else(|| image.pixels.clone(), |f| f.pixels.clone()),
        revision: revision.max(image.revision),
        frame: shown.as_ref().map_or(1, |f| f.frame),
        pixel_revision: shown.as_ref().map_or(image.revision, |f| f.revision),
        animation: image.anim.as_deref().map(super::Anim::state),
    }
}
