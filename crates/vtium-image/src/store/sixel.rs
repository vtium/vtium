//! A new picture over older ones of its kind, cell by cell (ADR-1100, D2
//! and D3, the rules table's row "A new picture of the same tag over it").
//!
//! When a picture whose rules say a new picture covers older ones (a
//! sixel's) is placed, the older pictures of its kind lose their parts
//! under every cell the new one covers fully with opaque pixels, as a
//! character written there would cut them. Under a cell it covers only
//! partly, because the cell is at the picture's edge or holds a clear
//! pixel of it, they stay and show through. Then each older picture of
//! its kind under the new one's box that draws nothing is removed too: one
//! left with no pixel set, or one whose every set pixel lies under an
//! opaque pixel of the new picture, so that it shows nowhere. This is the
//! per-cell coverage the operator chose after chafa's
//! frames, padded with clear pixels, stacked on each other until the
//! budget refused the next. The second kind of piece that draws nothing
//! is vtium's reading: without it, two pictures of the same size that end
//! inside a cell, as most of mpv's frames do, leave the older one's pieces
//! in the edge cells, under the new one's opaque pixels, and every frame
//! adds pieces and keeps an image until the budget refuses the next.
//!
//! Only a picture drawn at its own size can be read cell by cell. A
//! picture scaled into its box covers a cell fully when the cell lies
//! inside it and every pixel of the image is opaque, and is never taken
//! for empty.
//!
//! A picture with frames, an iTerm2 picture's animated GIF, shows any of
//! them as the animation clock picks (ADR-1103, D4). It covers a cell
//! fully only when every one of its frames covers that cell fully, hides a
//! pixel beneath only where every frame is opaque, and draws nothing in a
//! place only when every frame draws nothing there.

use super::{FramePixels, Geometry, Placement, Rows, Store};
use crate::image::{ImageKey, Place, PlacementKey};
use crate::rules::{Protocol, rules, same};

/// Where a placement's picture is drawn, in pixels from its box's top
/// left, and the image pixel at the drawn rectangle's top left.
#[derive(Clone, Copy, Debug)]
struct Drawn {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    sx: i64,
    sy: i64,
    /// Drawn at another size than the source, so a box pixel is no one
    /// image pixel.
    scaled: bool,
}

impl Drawn {
    fn new(g: &Geometry, place: &Place) -> Self {
        Self {
            x: g.ox,
            y: g.oy,
            w: i64::try_from(g.dw).unwrap_or(i64::MAX),
            h: i64::try_from(g.dh).unwrap_or(i64::MAX),
            sx: i64::from(place.src_x),
            sy: i64::from(place.src_y),
            scaled: g.dw != u64::from(g.sw) || g.dh != u64::from(g.sh),
        }
    }

    fn of(p: &Placement) -> Self {
        Self {
            x: p.ox,
            y: p.oy,
            w: i64::try_from(p.dw).unwrap_or(i64::MAX),
            h: i64::try_from(p.dh).unwrap_or(i64::MAX),
            sx: i64::from(p.sx),
            sy: i64::from(p.sy),
            scaled: p.dw != u64::from(p.sw) || p.dh != u64::from(p.sh),
        }
    }
}

/// A rectangle of box pixels, `x0..x1` by `y0..y1`.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
}

impl Rect {
    fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    fn inside(&self, d: &Drawn) -> bool {
        self.x0 >= d.x
            && self.y0 >= d.y
            && self.x1 <= d.x.saturating_add(d.w)
            && self.y1 <= d.y.saturating_add(d.h)
    }

    fn clip(&self, d: &Drawn) -> Self {
        Self {
            x0: self.x0.max(d.x),
            y0: self.y0.max(d.y),
            x1: self.x1.min(d.x.saturating_add(d.w)),
            y1: self.y1.min(d.y.saturating_add(d.h)),
        }
    }
}

/// One frame of a picture, as the rules here read it: its own size, its
/// pixels, and whether every one of them is opaque.
#[derive(Clone, Copy, Debug)]
struct Pic<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u32],
    opaque: bool,
}

/// Each frame of a picture `width` by `height`, from the frames the store
/// gives it, the image's own pixels first.
fn pics(frames: &[FramePixels], (width, height): (u32, u32)) -> Vec<Pic<'_>> {
    frames
        .iter()
        .map(|f| Pic {
            width,
            height,
            pixels: f.pixels.as_slice(),
            opaque: f.opaque,
        })
        .collect()
}

/// The image pixels of a picture drawn at its own size under box row `y`,
/// from box column `x0` to `x1`: one row of the image, or nothing where
/// the row lies outside it.
fn row_of<'a>(img: &Pic<'a>, d: &Drawn, y: i64, x0: i64, x1: i64) -> &'a [u32] {
    let width = i64::from(img.width);
    let row = d.sy.saturating_add(y.saturating_sub(d.y));
    let at = |x: i64| {
        let col = d.sx.saturating_add(x.saturating_sub(d.x));
        usize::try_from(row.saturating_mul(width).saturating_add(col)).ok()
    };
    if !(0..i64::from(img.height)).contains(&row) {
        return &[];
    }
    match (at(x0), at(x1)) {
        (Some(a), Some(b)) => img.pixels.get(a..b).unwrap_or_default(),
        _ => &[],
    }
}

/// Reads the image pixels under box rectangle `r` of a picture drawn at
/// its own size, row by row, until `stop` says so or the pixels read reach
/// `cap`; returns whether it stopped, and adds the pixels it read to
/// `read`.
fn scan(
    img: &Pic<'_>,
    d: &Drawn,
    r: Rect,
    read: &mut usize,
    cap: usize,
    stop: impl Fn(u32) -> bool,
) -> bool {
    for y in r.y0..r.y1 {
        let line = row_of(img, d, y, r.x0, r.x1);
        *read = read.saturating_add(line.len());
        if line.iter().any(|&px| stop(px)) || *read >= cap {
            return true;
        }
    }
    false
}

/// The pixel rectangle of box cell `col`, `row` in cells of `cw` by `ch`.
fn cell(col: u32, row: u32, (cw, ch): (u32, u32)) -> Rect {
    let x0 = i64::from(col).saturating_mul(i64::from(cw));
    let y0 = i64::from(row).saturating_mul(i64::from(ch));
    Rect {
        x0,
        y0,
        x1: x0.saturating_add(i64::from(cw)),
        y1: y0.saturating_add(i64::from(ch)),
    }
}

/// The runs of a new placement's box cells its picture covers fully with
/// opaque pixels in every frame, as (box row, first column, last column),
/// and the pixels read to find them.
fn covered(
    frames: &[Pic<'_>],
    d: &Drawn,
    g: &Geometry,
    size: (u32, u32),
) -> (Vec<(u32, u32, u32)>, usize) {
    let mut runs = Vec::new();
    let mut read = 0usize;
    // a cell is covered only inside the drawn picture, and only a picture
    // drawn at its own size can be read cell by cell
    let opaque = frames.iter().all(|f| f.opaque);
    if !opaque && d.scaled {
        return (runs, read);
    }
    for row in 0..g.rows {
        let mut start: Option<u32> = None;
        for col in 0..=g.cols {
            let full = col < g.cols && {
                let r = cell(col, row, size);
                r.inside(d)
                    && frames.iter().all(|f| {
                        f.opaque || !scan(f, d, r, &mut read, usize::MAX, |px| px >> 24 != 0xff)
                    })
            };
            match (full, start) {
                (true, None) => start = Some(col),
                (false, Some(first)) => {
                    runs.push((row, first, col.saturating_sub(1)));
                    start = None;
                }
                _ => {}
            }
        }
    }
    (runs, read)
}

impl Store {
    /// Removes what the picture `image` of buffer `b`, to be placed as
    /// `place` with geometry `g`, covers of the older pictures of its kind
    /// on `rows`, on a screen `cols` wide, and then each of them under its
    /// box left with no pixel set. The pixels read count as work (D5).
    pub(super) fn cover(
        &mut self,
        b: usize,
        image: ImageKey,
        place: &Place,
        g: &Geometry,
        rows: Rows<'_>,
        cols: usize,
    ) {
        let Some(img) = self.image(b, image) else {
            return;
        };
        let (protocol, size) = (img.protocol.0, (img.width, img.height));
        let frames = self.frames_of(image);
        let (runs, read) = covered(&pics(&frames, size), &Drawn::new(g, place), g, self.cell);
        self.count(read);
        let height = u32::try_from(rows.screen.len()).unwrap_or(u32::MAX);
        for (row, first, last) in runs {
            let y = place.row.saturating_add(row);
            if y >= height {
                break;
            }
            let (x0, x1) = (
                place.col.saturating_add(first),
                place.col.saturating_add(last),
            );
            self.cut_where(y, x0, x1, rows, cols, |p| same(p.protocol.0, protocol));
        }
        self.drop_hidden(b, image, place, g, rows, protocol);
    }

    /// Removes the placements of `protocol`'s kind under the box `g` gives
    /// at `place` that draw nothing where the picture `image` is to be
    /// drawn there: no pixel set, or every set pixel under an opaque pixel
    /// of `image`.
    fn drop_hidden(
        &mut self,
        b: usize,
        image: ImageKey,
        place: &Place,
        g: &Geometry,
        rows: Rows<'_>,
        protocol: Protocol,
    ) {
        let under = self.under(b, place, g, rows, protocol);
        let Some(top) = self.image(b, image) else {
            return;
        };
        let top_frames = self.frames_of(image);
        let over = Over {
            frames: pics(&top_frames, (top.width, top.height)),
            drawn: Drawn::new(g, place),
            col: place.col,
            row: i64::from(place.row),
            cell: self.cell,
        };
        // the check reads at most as many pixels as the new picture has,
        // so a placement costs at most twice its own pixels, whatever lies
        // under it; a piece left unchecked stays, which changes no pixel
        let cap = usize::try_from(g.dw.saturating_mul(g.dh)).unwrap_or(usize::MAX);
        let mut read = 0usize;
        let mut hidden = Vec::new();
        for (key, row) in under {
            if read >= cap {
                break;
            }
            let Some(p) = self.placement(b, key) else {
                continue;
            };
            let Some(img) = self.image(b, p.image) else {
                continue;
            };
            let frames = self.frames_of(p.image);
            let shows = pics(&frames, (img.width, img.height))
                .iter()
                .any(|f| shows_past(f, p, row, &over, &mut read, cap));
            if !shows {
                hidden.push(key);
            }
        }
        self.count(read);
        for key in hidden {
            self.free_placement(b, key, true);
        }
    }

    /// Every frame of image `key`, its own pixels first: one for a still
    /// picture.
    fn frames_of(&self, key: ImageKey) -> Vec<FramePixels> {
        let count = self.animation(key).map_or(1, |a| a.frames.max(1));
        (1..=count)
            .filter_map(|n| self.frame_pixels(key, n))
            .collect()
    }

    /// The placements of `protocol`'s kind whose box meets the box `g`
    /// gives at `place`, each with the screen row of its top, in the order
    /// of their lines from the top and of their arrival on each.
    fn under(
        &mut self,
        b: usize,
        place: &Place,
        g: &Geometry,
        rows: Rows<'_>,
        protocol: Protocol,
    ) -> Vec<(PlacementKey, i64)> {
        let Some(buf) = self.buffers.get(b) else {
            return Vec::new();
        };
        let (tallest, _) = buf.tallest();
        let top = i64::from(place.row);
        let bottom = top.saturating_add(i64::from(g.rows));
        let (left, right) = (place.col, place.col.saturating_add(g.cols));
        let mut keys = Vec::new();
        let mut visited = 0usize;
        // a placement under the box is anchored at most the tallest of
        // them, less one row, above its top
        let first = top.saturating_sub(i64::from(tallest)).saturating_add(1);
        for y in first..bottom.min(rows.height()) {
            let Some(line) = rows.at(y) else {
                continue;
            };
            visited = visited.saturating_add(1);
            for &key in buf.by_line.get(&line).into_iter().flatten() {
                visited = visited.saturating_add(1);
                let Some(p) = buf.placements.get(&key) else {
                    continue;
                };
                let meets = y.saturating_add(i64::from(p.rows)) > top
                    && p.col < right
                    && p.col.saturating_add(p.cols) > left;
                if meets && same(p.protocol.0, protocol) && rules(p.protocol.0).covered_by_same {
                    keys.push((key, y));
                }
            }
        }
        self.count(visited);
        keys
    }
}

/// The new picture an older one may lie under: its frames, where it is
/// drawn in its box, its box's screen column and row, and the cell size.
struct Over<'a> {
    frames: Vec<Pic<'a>>,
    drawn: Drawn,
    col: u32,
    row: i64,
    cell: (u32, u32),
}

/// Whether placement `p` of `img`, one frame of its picture, its top on
/// screen row `row`, shows a pixel once `over` is drawn: a pixel inside its
/// box with alpha above 0 that no pixel of `over` opaque in every frame
/// hides. A picture drawn
/// scaled, or in another cell size than `over`, is taken to show one, and
/// so is one whose check would read `read` past `cap`.
fn shows_past(
    img: &Pic<'_>,
    p: &Placement,
    row: i64,
    over: &Over<'_>,
    read: &mut usize,
    cap: usize,
) -> bool {
    let d = Drawn::of(p);
    let corner = cell(p.cols, p.rows, (p.cw, p.ch));
    let boxed = Rect {
        x0: 0,
        y0: 0,
        x1: corner.x0,
        y1: corner.y0,
    };
    let visible = boxed.clip(&d);
    if visible.is_empty() {
        return false;
    }
    if d.scaled || (p.cw, p.ch) != over.cell {
        return true;
    }
    // the new picture's drawn rectangle, in this placement's box pixels
    let (cw, ch) = (i64::from(over.cell.0), i64::from(over.cell.1));
    let dx = i64::from(over.col)
        .saturating_sub(i64::from(p.col))
        .saturating_mul(cw);
    let dy = over.row.saturating_sub(row).saturating_mul(ch);
    let o = &over.drawn;
    let covering = Rect {
        x0: dx.saturating_add(o.x),
        y0: dy.saturating_add(o.y),
        x1: dx.saturating_add(o.x).saturating_add(o.w),
        y1: dy.saturating_add(o.y).saturating_add(o.h),
    };
    // the parts outside the new picture show whatever is set there
    let outside = [
        Rect {
            y1: visible.y1.min(covering.y0),
            ..visible
        },
        Rect {
            y0: visible.y0.max(covering.y1),
            ..visible
        },
        Rect {
            y0: visible.y0.max(covering.y0),
            y1: visible.y1.min(covering.y1),
            x1: visible.x1.min(covering.x0),
            ..visible
        },
        Rect {
            y0: visible.y0.max(covering.y0),
            y1: visible.y1.min(covering.y1),
            x0: visible.x0.max(covering.x1),
            ..visible
        },
    ];
    for r in outside.iter().filter(|r| !r.is_empty()) {
        if img.opaque || scan(img, &d, *r, read, cap, |px| px >> 24 != 0) {
            return true;
        }
    }
    // under the new picture, a set pixel shows through a clear one of it
    let inside = Rect {
        x0: visible.x0.max(covering.x0),
        y0: visible.y0.max(covering.y0),
        x1: visible.x1.min(covering.x1),
        y1: visible.y1.min(covering.y1),
    };
    if inside.is_empty() || over.frames.iter().all(|f| f.opaque) {
        return false;
    }
    if o.scaled {
        return true;
    }
    for y in inside.y0..inside.y1 {
        let mine = row_of(img, &d, y, inside.x0, inside.x1);
        *read = read.saturating_add(mine.len());
        // a set pixel shows where any frame of the new picture is clear
        for f in &over.frames {
            let theirs = row_of(
                f,
                o,
                y.saturating_sub(dy),
                inside.x0.saturating_sub(dx),
                inside.x1.saturating_sub(dx),
            );
            if mine.len() != theirs.len()
                || mine
                    .iter()
                    .zip(theirs)
                    .any(|(&m, &t)| m >> 24 != 0 && t >> 24 != 0xff)
            {
                return true;
            }
        }
        if *read >= cap {
            return true;
        }
    }
    false
}
