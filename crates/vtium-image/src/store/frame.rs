//! The frame's placement list (ADR-1100, D9).

use std::collections::BTreeMap;

use super::{Located, Placement, Rows, Store};
use crate::image::{ImageKey, LineKey, Pixels, PlacementKey};
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
    /// No pixel of the image is below full alpha.
    pub opaque: bool,
    pub pixels: Pixels,
    /// The store's [`revision`](Store::revision) when the placement or its
    /// image last changed.
    pub revision: u64,
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
        let screen = view.screen.iter().zip(0i64..);
        let above = view.above.iter().zip((1i64..).map(i64::wrapping_neg));
        let mut found: Vec<(usize, i64, PlacementKey)> = Vec::new();
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
                    found.push((order.get(&k).copied().unwrap_or(usize::MAX), row, k));
                }
            }
        }
        let height = i64::try_from(view.screen.len()).unwrap_or(i64::MAX);
        if !buf.children.is_empty() {
            for (line, row) in below.iter().zip(height..) {
                line_rows.insert(*line, row);
            }
        }
        let width = i64::try_from(cols).unwrap_or(i64::MAX);
        let mut moved: BTreeMap<PlacementKey, (Placement, u64)> = BTreeMap::new();
        for &k in buf.children.values().flatten() {
            let (
                Some(p),
                Some(Located::At {
                    line,
                    row,
                    col,
                    revision,
                    ..
                }),
            ) = (buf.placements.get(&k), buf.locate(k))
            else {
                continue;
            };
            let Some(top) = line_rows.get(&line).map(|r| r.saturating_add(row)) else {
                continue;
            };
            let shows = top.saturating_add(i64::from(p.rows)) > 0 && top < height && col < width;
            let Some(placed) = shows.then(|| at_column(p, col)).flatten() else {
                continue;
            };
            found.push((order.get(&k).copied().unwrap_or(usize::MAX), top, k));
            moved.insert(k, (placed, revision));
        }
        found.sort_unstable();
        found
            .into_iter()
            .filter_map(|(_, row, k)| {
                let (p, revision) = match moved.get(&k) {
                    Some((p, revision)) => (p, *revision),
                    None => {
                        let p = buf.placements.get(&k)?;
                        (p, p.revision)
                    }
                };
                let image = buf.images.get(&p.image)?;
                Some(entry(k, p, image, row, revision))
            })
            .collect()
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
        opaque: image.opaque,
        pixels: image.pixels.clone(),
        revision: revision.max(image.revision),
    }
}
