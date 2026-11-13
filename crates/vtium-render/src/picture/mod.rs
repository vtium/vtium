//! Pictures in the CPU compositor (ADR-1100 D4, D5, D9; issue #30): where
//! each of a frame's placements draws on the display, the scaled copies
//! each display keeps, and the cells a picture that moved or changed
//! draws again.
//!
//! The layers a cell under a picture is composed in are the row scratch's
//! (`src/compose.rs`); this module says which pictures reach a cell and
//! hands each one's pixels a display line at a time.
// Pictures scale to the same pixels on every machine (docs/architecture.md,
// "Determinism"): integer arithmetic only, here and in every submodule.
#![deny(clippy::float_arithmetic)]

pub(crate) mod copies;
pub(crate) mod scale;

use std::collections::BTreeMap;

use vtium_image::{FramePlacement, Layer, Pixels, PlacementKey};

use crate::blend::div255;
use crate::compose::Area;
use crate::target::Orientation;

use copies::Copies;
pub(crate) use copies::Key;
pub use copies::PictureCounts;
use scale::{Source, Taps};

/// How strongly the selection's color tints a selected cell that a
/// picture reaches, out of 255: the selected cell shows both the selection
/// and the picture under it (the maintainer, 2026-11-01).
pub const SELECTION_TINT: u8 = 128;

/// What a cell's own state adds to the pictures over it: whether its
/// background is the default, which lets the layer below backgrounds show
/// (ADR-1001's `default_bg`), whether it is selected, and the cursor
/// vtium draws over it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Under {
    pub(crate) default_bg: bool,
    pub(crate) selected: bool,
    pub(crate) mark: Mark,
}

/// vtium's mark on a cell, which draws above every picture (ADR-1100 D4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mark {
    #[default]
    None,
    /// A block cursor: the cell, drawn as without pictures, covers every
    /// picture under it.
    Opaque,
    /// A bar cursor: the cell's glyph is the bar.
    Bar,
    /// An underline cursor: the cell's underline is the cursor's.
    Underline,
}

/// A placement as the last frame drew it, for the damage of the next: its
/// box, its picture and the pixels it showed, named by the image, the
/// frame and its pixel revision, so a placement that moves to another
/// frame, or whose frame a command changed, draws its cells again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Drawn {
    row: i64,
    col: u32,
    cols: u32,
    rows: u32,
    part: (u32, u32, u32, u32),
    src: (u32, u32, u32, u32),
    cell: (u32, u32),
    layer: Layer,
    z: i32,
    image: u64,
    revision: u64,
    frame: u32,
    pixel_revision: u64,
}

impl Drawn {
    fn of(p: &FramePlacement) -> Drawn {
        Drawn {
            row: p.row,
            col: p.col,
            cols: p.cols,
            rows: p.rows,
            part: (p.x, p.y, p.width, p.height),
            src: (p.src_x, p.src_y, p.src_width, p.src_height),
            cell: (p.cell_width, p.cell_height),
            layer: p.layer,
            z: p.z,
            image: p.image.get(),
            revision: p.revision,
            frame: p.frame,
            pixel_revision: p.pixel_revision,
        }
    }
}

/// How a picture's pixels are had.
#[derive(Debug)]
enum How {
    /// Read from the image's pixels as they are, drawn at its own size,
    /// with turned indexes on a turned display.
    Direct,
    /// From the kept copy of its key, once a cell under it is drawn.
    Pending,
    Copy,
    /// Scaled for this frame alone, the copy being past the cache's room:
    /// the weights for its columns `u0` on and its rows `v0` on.
    Scaled {
        tx: Taps,
        ty: Taps,
    },
}

/// One of the frame's pictures.
#[derive(Debug)]
pub(crate) struct Pic {
    layer: Layer,
    opaque: bool,
    /// The whole picture's display rectangle.
    rect: Area,
    /// The part it draws, in display pixels: its visible part, inside its
    /// box and the grid.
    clip: Area,
    key: Key,
    pixels: Pixels,
    how: How,
}

impl Pic {
    /// Whether the picture draws over the whole of `a`.
    pub(crate) fn covers(&self, a: &Area) -> bool {
        self.clip.x <= a.x
            && self.clip.y <= a.y
            && right(&self.clip) >= right(a)
            && self.clip.bottom() >= a.bottom()
    }

    pub(crate) fn meets(&self, a: &Area) -> bool {
        self.clip.x < right(a)
            && a.x < right(&self.clip)
            && self.clip.y < a.bottom()
            && a.y < self.clip.bottom()
    }

    pub(crate) fn layer(&self) -> Layer {
        self.layer
    }

    pub(crate) fn opaque(&self) -> bool {
        self.opaque
    }

    /// The whole picture's display rectangle.
    #[cfg(feature = "gpu")]
    pub(crate) fn rect(&self) -> Area {
        self.rect
    }

    /// The part it draws, in display pixels.
    #[cfg(feature = "gpu")]
    pub(crate) fn clip(&self) -> Area {
        self.clip
    }

    /// What names its pixels, the part of them it shows and its drawn size.
    #[cfg(feature = "gpu")]
    pub(crate) fn key(&self) -> Key {
        self.key
    }

    /// The image's pixels.
    #[cfg(feature = "gpu")]
    pub(crate) fn pixels(&self) -> &Pixels {
        &self.pixels
    }
}

fn right(a: &Area) -> i64 {
    a.x.saturating_add(a.w)
}

fn meet(a: &Area, b: &Area) -> Area {
    let (x, y) = (a.x.max(b.x), a.y.max(b.y));
    Area {
        x,
        y,
        w: right(a).min(right(b)).saturating_sub(x).max(0),
        h: a.bottom().min(b.bottom()).saturating_sub(y).max(0),
    }
}

fn turn(o: Orientation) -> u8 {
    match o {
        Orientation::Normal => 0,
        Orientation::Right => 1,
        Orientation::UpsideDown => 2,
        Orientation::Left => 3,
    }
}

/// Where a placement's picture lies in the grid's pixels, unturned.
struct Geometry {
    /// The whole picture.
    pic: Area,
    /// The part it draws.
    clip: Area,
    /// The source rectangle the whole picture shows: its crop.
    src: (u32, u32, u32, u32),
    /// Drawn at the crop's own size, in the renderer's cell.
    own_size: bool,
}

/// `p`'s picture at the renderer's cell `cell`, on a grid `grid` pixels
/// wide and tall; `None` when it draws nothing or is malformed.
///
/// A placement's pixel values are in the cell it was made in, and a value
/// `m` measured from the grid's origin lands at `floor(m * cell / made)`,
/// so the pieces of one picture meet with no gap and no overlap at any
/// cell size (ADR-1100 D8, "A cell size change").
fn geometry(p: &FramePlacement, (cw, ch): (u32, u32), (gw, gh): (i64, i64)) -> Option<Geometry> {
    let (mw, mh) = (i64::from(p.cell_width), i64::from(p.cell_height));
    let (iw, ih) = (p.image_width, p.image_height);
    let pixels = u64::from(iw).checked_mul(u64::from(ih))?;
    let fits = |at: u32, n: u32, side: u32| at.checked_add(n).is_some_and(|end| end <= side);
    let well_formed = mw > 0
        && mh > 0
        && p.width > 0
        && p.height > 0
        && p.crop_width > 0
        && p.crop_height > 0
        && p.cols > 0
        && p.rows > 0
        && u64::try_from(p.pixels.as_slice().len()).ok() == Some(pixels)
        && fits(p.crop_x, p.crop_width, iw)
        && fits(p.crop_y, p.crop_height, ih);
    let (Ok(dw), Ok(dh)) = (i64::try_from(p.drawn_width), i64::try_from(p.drawn_height)) else {
        return None;
    };
    if !well_formed || dw <= 0 || dh <= 0 {
        return None;
    }
    let (cw, ch) = (i64::from(cw), i64::from(ch));
    let to =
        |m: i64, cell: i64, made: i64| m.saturating_mul(cell).checked_div_euclid(made).unwrap_or(0);
    let span = |a: i64, b: i64, cell: i64, made: i64| {
        let (lo, hi) = (to(a, cell, made), to(b, cell, made));
        (lo, hi.saturating_sub(lo))
    };
    // the visible part's top left, in made pixels from the grid's origin
    let mx = i64::from(p.col)
        .saturating_mul(mw)
        .saturating_add(i64::from(p.x));
    let my = p.row.saturating_mul(mh).saturating_add(i64::from(p.y));
    let (vx, vw) = span(mx, mx.saturating_add(i64::from(p.width)), cw, mw);
    let (vy, vh) = span(my, my.saturating_add(i64::from(p.height)), ch, mh);
    let visible = Area {
        x: vx,
        y: vy,
        w: vw,
        h: vh,
    };
    let in_box = Area {
        x: i64::from(p.col).saturating_mul(cw),
        y: p.row.saturating_mul(ch),
        w: i64::from(p.cols).saturating_mul(cw),
        h: i64::from(p.rows).saturating_mul(ch),
    };
    let grid = Area {
        x: 0,
        y: 0,
        w: gw,
        h: gh,
    };
    // the whole drawn picture, its crop at its scale: every piece a cut or
    // a scroll leaves of one placement names the same picture, so they
    // share one copy, and the copy reads nothing past the crop
    let ox = i64::from(p.col)
        .saturating_mul(mw)
        .saturating_add(p.offset_x);
    let oy = p.row.saturating_mul(mh).saturating_add(p.offset_y);
    let (x, w) = span(ox, ox.saturating_add(dw), cw, mw);
    let (y, h) = span(oy, oy.saturating_add(dh), ch, mh);
    let pic = Area { x, y, w, h };
    let src = (p.crop_x, p.crop_y, p.crop_width, p.crop_height);
    let own_size =
        p.drawn_width == u64::from(p.crop_width) && p.drawn_height == u64::from(p.crop_height);
    let clip = meet(&meet(&meet(&visible, &in_box), &grid), &pic);
    if clip.w <= 0 || clip.h <= 0 || pic.w <= 0 || pic.h <= 0 {
        return None;
    }
    Some(Geometry {
        pic,
        clip,
        src,
        own_size: own_size && cw == mw && ch == mh,
    })
}

/// The part of a placement's picture that the renderer draws, and its
/// pixels: `width` by `height` display pixels at `x`, `y` from the grid's
/// top left, premultiplied ARGB, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedPicture {
    pub x: i64,
    pub y: i64,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

/// The part of `p`'s picture that a renderer with cells of `cell` draws on
/// a grid `grid` pixels wide and tall, with the pixels it draws there: the
/// crop scaled to the whole picture's display size as a frame scales it,
/// then cut to the part drawn. `None` when it draws nothing. A tool that
/// sends a picture at its display size, as `make tier1-play` does, sends
/// these pixels, and a renderer draws them at their own size.
pub fn placed_picture(
    p: &FramePlacement,
    cell: (u32, u32),
    (gw, gh): (u32, u32),
) -> Option<PlacedPicture> {
    let g = geometry(p, cell, (i64::from(gw), i64::from(gh)))?;
    let (sx, sy, sw, sh) = g.src;
    let (pw, ph) = (u32::try_from(g.pic.w).ok()?, u32::try_from(g.pic.h).ok()?);
    let src = Source {
        pixels: p.pixels.as_slice(),
        stride: usize::try_from(p.image_width).ok()?,
        x: sx,
        y: sy,
    };
    let whole = scale::turned_copy(&src, (sw, sh), (pw, ph), Orientation::Normal);
    let (cx, cy) = (
        g.clip.x.checked_sub(g.pic.x)?,
        g.clip.y.checked_sub(g.pic.y)?,
    );
    let (width, height) = (u32::try_from(g.clip.w).ok()?, u32::try_from(g.clip.h).ok()?);
    let stride = usize::try_from(pw).ok()?;
    let mut pixels =
        Vec::with_capacity(usize::try_from(u64::from(width).checked_mul(u64::from(height))?).ok()?);
    for row in 0..i64::from(height) {
        let start = usize::try_from(cy.checked_add(row)?)
            .ok()?
            .checked_mul(stride)?;
        let start = start.checked_add(usize::try_from(cx).ok()?)?;
        let end = start.checked_add(usize::try_from(width).ok()?)?;
        pixels.extend_from_slice(whole.get(start..end)?);
    }
    Some(PlacedPicture {
        x: g.clip.x,
        y: g.clip.y,
        width,
        height,
        pixels,
    })
}

/// The pictures of the frame being drawn, the copies kept between frames,
/// and the placements the last frame drew.
#[derive(Debug)]
pub(crate) struct Pictures {
    copies: Copies,
    orientation: Orientation,
    pics: Vec<Pic>,
    /// Each grid row's pictures, in draw order: the pictures from
    /// `row_first[r]` to `row_first[r + 1]` in `row_pics`.
    row_first: Vec<usize>,
    row_pics: Vec<usize>,
    /// The grid row being drawn.
    pub(crate) row: usize,
    last: BTreeMap<PlacementKey, Drawn>,
    /// A line of a picture scaled for one frame.
    line: Vec<u32>,
    /// Runs of display pixels drawn, for [`PictureCounts::lines`].
    lines: u64,
}

impl Pictures {
    /// No pictures, until [`Pictures::display`] says what they draw on.
    pub(crate) fn new() -> Pictures {
        Pictures {
            copies: Copies::new(0),
            orientation: Orientation::Normal,
            pics: Vec::new(),
            row_first: Vec::new(),
            row_pics: Vec::new(),
            row: 0,
            last: BTreeMap::new(),
            line: Vec::new(),
            lines: 0,
        }
    }

    /// The display the pictures draw on: `width` by `height` pixels,
    /// turned as `orientation` says, whose copies hold at most
    /// [`crate::PICTURE_CACHE_SCREENS`] screens of 4-byte pixels. A
    /// renderer's display never changes, so each frame sets the same.
    pub(crate) fn display(&mut self, width: u32, height: u32, orientation: Orientation) {
        let screen = u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(4);
        let screens = u64::try_from(crate::bounds::PICTURE_CACHE_SCREENS).unwrap_or(u64::MAX);
        self.copies.set_limit(screen.saturating_mul(screens));
        self.orientation = orientation;
    }

    pub(crate) fn counts(&self) -> PictureCounts {
        PictureCounts {
            lines: self.lines,
            ..self.copies.counts()
        }
    }

    /// The cells whose pictures changed since the last frame: each cell,
    /// of a grid `cols` by `rows`, of the old and the new box of every
    /// placement that arrived, left or changed, goes to `stamp`. A picture
    /// that moved or changed redraws only what it covers, so a pane whose
    /// picture stayed is not drawn again (issue #30).
    pub(crate) fn damage(
        &mut self,
        placements: &[FramePlacement],
        (cols, rows): (usize, usize),
        mut stamp: impl FnMut(usize, usize),
    ) {
        let mut now = BTreeMap::new();
        for p in placements {
            now.insert(p.key, Drawn::of(p));
        }
        let mut cells = |d: &Drawn| {
            let r0 = d.row.max(0);
            let r1 = d.row.saturating_add(i64::from(d.rows));
            let rows = usize::try_from(r0).unwrap_or(usize::MAX)
                ..usize::try_from(r1.max(0)).unwrap_or(0).min(rows);
            let c0 = usize::try_from(d.col).unwrap_or(usize::MAX);
            let c1 = usize::try_from(d.col.saturating_add(d.cols))
                .unwrap_or(usize::MAX)
                .min(cols);
            for r in rows {
                for c in c0..c1 {
                    stamp(c, r);
                }
            }
        };
        for (k, d) in &now {
            if self.last.get(k) != Some(d) {
                cells(d);
            }
        }
        for (k, d) in &self.last {
            if now.get(k) != Some(d) {
                cells(d);
            }
        }
        self.last = now;
    }

    /// Plans the frame's pictures, in the order `placements` hands them,
    /// for a renderer of cell `cell` on a grid `grid` pixels wide and tall
    /// of `rows` rows; `display` takes an area of the grid's pixels to the
    /// display's. A copy a placement shows is kept for the frame.
    pub(crate) fn plan(
        &mut self,
        placements: &[FramePlacement],
        cell: (u32, u32),
        grid: (i64, i64),
        rows: usize,
        display: impl Fn(Area) -> Area,
    ) {
        self.copies.begin_frame();
        self.pics.clear();
        let turn = turn(self.orientation);
        let mut spans = Vec::new();
        for p in placements {
            let Some(g) = geometry(p, cell, grid) else {
                continue;
            };
            let size = (
                u32::try_from(g.pic.w).unwrap_or(u32::MAX),
                u32::try_from(g.pic.h).unwrap_or(u32::MAX),
            );
            let key = Key {
                image: p.image.get(),
                frame: p.frame,
                pixel_revision: p.pixel_revision,
                image_size: (p.image_width, p.image_height),
                src: g.src,
                size,
                turn,
            };
            let how = if g.own_size {
                How::Direct
            } else if self.copies.place(&key) {
                How::Copy
            } else {
                How::Pending
            };
            let ch = i64::from(cell.1).max(1);
            let r0 = g.clip.y.checked_div(ch).unwrap_or(0);
            let r1 = g
                .clip
                .bottom()
                .saturating_add(ch.saturating_sub(1))
                .checked_div(ch)
                .unwrap_or(0);
            spans.push((
                usize::try_from(r0.max(0)).unwrap_or(0),
                usize::try_from(r1.max(0)).unwrap_or(0).min(rows),
            ));
            self.pics.push(Pic {
                layer: p.layer,
                opaque: p.opaque,
                rect: display(g.pic),
                clip: display(g.clip),
                key,
                pixels: p.pixels.clone(),
                how,
            });
        }
        // each row's pictures, in draw order
        self.row_first.clear();
        self.row_first.resize(rows.saturating_add(1), 0);
        for &(r0, r1) in &spans {
            for r in r0..r1 {
                if let Some(n) = self.row_first.get_mut(r) {
                    *n = n.saturating_add(1);
                }
            }
        }
        let mut at = 0usize;
        for n in &mut self.row_first {
            let count = *n;
            *n = at;
            at = at.saturating_add(count);
        }
        self.row_pics.clear();
        self.row_pics.resize(at, 0);
        let mut next = self.row_first.clone();
        for (i, &(r0, r1)) in spans.iter().enumerate() {
            for r in r0..r1 {
                if let Some(slot) = next.get_mut(r) {
                    if let Some(s) = self.row_pics.get_mut(*slot) {
                        *s = i;
                    }
                    *slot = slot.saturating_add(1);
                }
            }
        }
    }

    /// Lets the frame's pictures go, so a deleted image's pixels outlive
    /// it only while a frame holds them (ADR-1100 D9).
    pub(crate) fn end_frame(&mut self) {
        self.pics.clear();
    }

    /// The pictures of the row being drawn that draw on `a`, in draw
    /// order, into `out`.
    pub(crate) fn meeting(&self, a: &Area, out: &mut Vec<usize>) {
        out.clear();
        let first = self.row_first.get(self.row).copied().unwrap_or(0);
        let end = self
            .row_first
            .get(self.row.saturating_add(1))
            .copied()
            .unwrap_or(first);
        for &i in self.row_pics.get(first..end).unwrap_or_default() {
            if self.pics.get(i).is_some_and(|p| p.meets(a)) {
                out.push(i);
            }
        }
    }

    pub(crate) fn pic(&self, i: usize) -> Option<&Pic> {
        self.pics.get(i)
    }

    /// The frame's pictures, in draw order.
    #[cfg(feature = "gpu")]
    pub(crate) fn pics(&self) -> &[Pic] {
        &self.pics
    }

    /// Has picture `i`'s pixels ready: its kept copy, made now when there
    /// is room, or else the weights to scale it for this frame alone.
    pub(crate) fn ready(&mut self, i: usize) {
        let Some(pic) = self.pics.get_mut(i) else {
            return;
        };
        if !matches!(pic.how, How::Pending) {
            return;
        }
        let key = pic.key;
        let (iw, _) = key.image_size;
        let src = Source {
            pixels: pic.pixels.as_slice(),
            stride: usize::try_from(iw).unwrap_or(0),
            x: key.src.0,
            y: key.src.1,
        };
        let (sw, sh) = (key.src.2, key.src.3);
        if self.copies.place(&key) {
            pic.how = How::Copy;
        } else if self.copies.room(&key, key.bytes()) {
            let copy = scale::turned_copy(&src, (sw, sh), key.size, self.orientation);
            self.copies.keep(key, copy, key.bytes());
            // the CPU path's copies are its own memory, freed as they go
            drop(self.copies.take_gone());
            pic.how = How::Copy;
        } else {
            drop(self.copies.take_gone());
            self.copies.uncached();
            // the weights for the part it draws alone
            let (w, h) = key.size;
            let corner = |x: i64, y: i64| {
                let off = |v: i64, at: i64| u32::try_from(v.saturating_sub(at)).unwrap_or(0);
                scale::uv(
                    self.orientation,
                    (w, h),
                    (off(x, pic.rect.x), off(y, pic.rect.y)),
                )
            };
            let (ua, va) = corner(pic.clip.x, pic.clip.y);
            let (ub, vb) = corner(
                right(&pic.clip).saturating_sub(1),
                pic.clip.bottom().saturating_sub(1),
            );
            let end = |a: u32, b: u32| a.max(b).saturating_add(1);
            pic.how = How::Scaled {
                tx: Taps::new(sw, w, ua.min(ub), end(ua, ub)),
                ty: Taps::new(sh, h, va.min(vb), end(va, vb)),
            };
        }
    }

    /// Picture `i` over the display line `y` of `out`, whose first pixel
    /// is at display column `x0`, where it draws there: the premultiplied
    /// picture over the opaque pixels, taken as it is where it is opaque.
    pub(crate) fn over(&mut self, i: usize, y: i64, x0: i64, out: &mut [u32]) {
        let Some(pic) = self.pics.get(i) else {
            return;
        };
        if y < pic.clip.y || y >= pic.clip.bottom() {
            return;
        }
        let end = x0.saturating_add(i64::try_from(out.len()).unwrap_or(i64::MAX));
        let (a, b) = (x0.max(pic.clip.x), end.min(right(&pic.clip)));
        if a >= b {
            return;
        }
        let at = |v: i64| usize::try_from(v).unwrap_or(0);
        let Some(dst) = out.get_mut(at(a.saturating_sub(x0))..at(b.saturating_sub(x0))) else {
            return;
        };
        let (dx, dy) = (a.saturating_sub(pic.rect.x), y.saturating_sub(pic.rect.y));
        let n = dst.len();
        let row = |stride: i64| at(dy.saturating_mul(stride).saturating_add(dx));
        let src: &[u32] = match &pic.how {
            How::Direct if self.orientation == Orientation::Normal => {
                // the crop's pixel under the display's, in the image's rows
                let (cx, cy) = (i64::from(pic.key.src.0), i64::from(pic.key.src.1));
                let start = at(dy
                    .saturating_add(cy)
                    .saturating_mul(i64::from(pic.key.image_size.0))
                    .saturating_add(dx)
                    .saturating_add(cx));
                pic.pixels
                    .as_slice()
                    .get(start..start.saturating_add(n))
                    .unwrap_or_default()
            }
            How::Direct => {
                // turned: the crop's pixels under the display line, read
                // where the turn puts them, with no copy made. Along a
                // display line the image index moves by one fixed step: a
                // quarter turn right walks up a column of the image, a
                // quarter turn left down one, half a turn back along a row
                let px = pic.pixels.as_slice();
                let iw = i64::from(pic.key.image_size.0);
                let (cx, cy) = (i64::from(pic.key.src.0), i64::from(pic.key.src.1));
                let oy = u32::try_from(dy).unwrap_or(0);
                let ox = u32::try_from(dx).unwrap_or(0);
                let (u, v) = scale::uv(self.orientation, pic.key.size, (ox, oy));
                let step = match self.orientation {
                    Orientation::Right => iw.saturating_neg(),
                    Orientation::Left => iw,
                    Orientation::UpsideDown => -1,
                    Orientation::Normal => 1,
                };
                let first = cy
                    .saturating_add(i64::from(v))
                    .saturating_mul(iw)
                    .saturating_add(cx)
                    .saturating_add(i64::from(u));
                self.line.clear();
                self.line.extend((0..).take(n).map(|k: i64| {
                    let i = first.saturating_add(k.saturating_mul(step));
                    usize::try_from(i)
                        .ok()
                        .and_then(|i| px.get(i))
                        .copied()
                        .unwrap_or(0)
                }));
                &self.line
            }
            How::Copy => {
                let start = row(pic.rect.w);
                self.copies
                    .get(&pic.key)
                    .and_then(|c| c.get(start..start.saturating_add(n)))
                    .unwrap_or_default()
            }
            How::Scaled { tx, ty } => {
                let source = Source {
                    pixels: pic.pixels.as_slice(),
                    stride: usize::try_from(pic.key.image_size.0).unwrap_or(0),
                    x: pic.key.src.0,
                    y: pic.key.src.1,
                };
                self.line.clear();
                let oy = u32::try_from(dy).unwrap_or(0);
                for k in 0..n {
                    let ox = u32::try_from(dx.saturating_add(i64::try_from(k).unwrap_or(0)))
                        .unwrap_or(0);
                    let (u, v) = scale::uv(self.orientation, pic.key.size, (ox, oy));
                    self.line.push(scale::eval(&source, tx, ty, u, v));
                }
                &self.line
            }
            How::Pending => &[],
        };
        if src.len() != n {
            return;
        }
        self.lines = self.lines.saturating_add(1);
        if pic.opaque {
            // a display pixel is 0x00RRGGBB
            for (d, &s) in dst.iter_mut().zip(src) {
                *d = s & 0x00ff_ffff;
            }
        } else {
            for (d, &s) in dst.iter_mut().zip(src) {
                *d = over_px(*d, s);
            }
        }
    }
}

/// The selection's color `c` over the display pixel `dst` at
/// [`SELECTION_TINT`]: each channel `(c * t + dst * (255 - t)) / 255`,
/// rounded, the arithmetic a mask's coverage blends with.
pub(crate) fn tint(dst: u32, c: vtium_term::Rgb) -> u32 {
    let ch = |shift: u32| u8::try_from(dst >> shift & 0xff).unwrap_or(0);
    let under = vtium_term::Rgb::new(ch(16), ch(8), ch(0));
    crate::blend::mask(c, under, SELECTION_TINT)
}

/// The premultiplied `src` over the opaque display pixel `dst`: each
/// channel `src + dst * (255 - a) / 255`, the arithmetic a cell's text
/// layer goes over its background with (ADR-1300, "Carried forward").
/// A channel past 255, which only a malformed premultiplied pixel gives,
/// stops at 255.
pub(crate) fn over_px(dst: u32, src: u32) -> u32 {
    let a = src >> 24;
    match a {
        0 => dst,
        255 => src & 0x00ff_ffff,
        _ => {
            let na = 255u32.wrapping_sub(a);
            let ch = |shift: u32| {
                let s = src >> shift & 0xff;
                let d = dst >> shift & 0xff;
                s.wrapping_add(div255(d.wrapping_mul(na))).min(255) << shift
            };
            ch(16) | ch(8) | ch(0)
        }
    }
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use vtium_image::{LineKey, NewImage, Place, Protocol, Rows, Screens, Store};

    /// A 40x30 picture placed at column 1, row 1, 3 and 5 pixels into its
    /// cell, on a screen of 10 by 4 cells of 8x16.
    fn placements() -> Vec<FramePlacement> {
        let keys: Vec<LineKey> = (1..=4).map(LineKey::new).collect();
        let rows = Rows {
            screen: &keys,
            above: &[],
        };
        let screens = Screens {
            main: rows,
            alt: Rows::default(),
        };
        let mut store = Store::new();
        store.set_limit(1 << 24);
        let pixels = (0..30u32)
            .flat_map(|y| {
                (0..40u32).map(move |x| {
                    0xff00_0000 | x.wrapping_mul(6) << 16 | y.wrapping_mul(8) << 8 | x ^ y
                })
            })
            .collect();
        let new = NewImage {
            protocol: Protocol::SIXEL,
            width: 40,
            height: 30,
            pixels,
            id: 0,
            number: 0,
            transient: false,
            opaque: None,
        };
        let image = store.add_image(new, None, screens).expect("stored");
        let place = Place {
            col: 1,
            row: 1,
            x: 3,
            y: 5,
            ..Place::default()
        };
        store.place(image, place, screens, 10).expect("placed");
        store.frame(rows, 10)
    }

    /// Grid areas on a display the grid's size exactly, turned as `o` says,
    /// as the renderer's layout turns them.
    fn display(o: Orientation, (gw, gh): (i64, i64)) -> impl Fn(Area) -> Area {
        let back = |n: i64, at: i64, len: i64| n.saturating_sub(at).saturating_sub(len);
        move |a: Area| {
            let (x, y, w, h) = match o {
                Orientation::Normal => (a.x, a.y, a.w, a.h),
                Orientation::UpsideDown => (back(gw, a.x, a.w), back(gh, a.y, a.h), a.w, a.h),
                Orientation::Right => (back(gh, a.y, a.h), a.x, a.h, a.w),
                Orientation::Left => (a.y, back(gw, a.x, a.w), a.h, a.w),
            };
            Area { x, y, w, h }
        }
    }

    /// Every display line of the frame's one picture, drawn over black.
    fn lines(p: &mut Pictures) -> Vec<Vec<u32>> {
        p.ready(0);
        let clip = p.pic(0).expect("one picture").clip;
        let w = usize::try_from(clip.w).unwrap_or(0);
        (clip.y..clip.bottom())
            .map(|y| {
                let mut out = vec![0; w];
                p.over(0, y, clip.x, &mut out);
                out
            })
            .collect()
    }

    // a placed picture drawn at its own size is its pixels at its place,
    // and one drawn in larger cells is scaled to their size there
    #[test]
    fn a_placed_picture_is_the_part_drawn_with_the_pixels_drawn_there() {
        let all = placements();
        let p = all.first().expect("one placement");
        let own = placed_picture(p, (8, 16), (80, 64)).expect("drawn");
        assert_eq!((own.x, own.y, own.width, own.height), (11, 21, 40, 30));
        assert_eq!(own.pixels, p.pixels.as_slice());
        let big = placed_picture(p, (16, 32), (160, 128)).expect("drawn");
        assert_eq!((big.x, big.y, big.width, big.height), (22, 42, 80, 60));
        let src = Source {
            pixels: p.pixels.as_slice(),
            stride: 40,
            x: 0,
            y: 0,
        };
        let scaled = scale::turned_copy(&src, (40, 30), (80, 60), Orientation::Normal);
        assert_eq!(big.pixels, scaled);
        // cut by the grid's right edge: the part drawn, its pixels the
        // scaled picture's columns there
        let cut = placed_picture(p, (16, 32), (64, 128)).expect("drawn");
        assert_eq!((cut.x, cut.width, cut.height), (22, 42, 60));
        assert_eq!(cut.pixels.get(..42), scaled.get(..42));
        assert_eq!(cut.pixels.get(42..84), scaled.get(80..122));
    }

    // ADR-1100 D5: a copy that does not fit is drawn scaled for that frame,
    // so the cache changes speed and never pixels
    #[test]
    fn a_picture_scaled_for_one_frame_draws_its_kept_copys_pixels() {
        let placed = placements();
        for o in [
            Orientation::Normal,
            Orientation::Right,
            Orientation::UpsideDown,
            Orientation::Left,
        ] {
            for cell in [(8, 16), (12, 24), (5, 11)] {
                let grid = (10 * i64::from(cell.0), 4 * i64::from(cell.1));
                let mut kept = Pictures::new();
                kept.display(1000, 1000, o);
                let mut alone = Pictures::new();
                alone.display(0, 0, o);
                kept.plan(&placed, cell, grid, 4, display(o, grid));
                alone.plan(&placed, cell, grid, 4, display(o, grid));
                let (a, b) = (lines(&mut kept), lines(&mut alone));
                assert!(!a.is_empty());
                assert_eq!(a, b, "{o:?} at {cell:?}");
                // at its own size a picture is read from the image, turned or not
                let own = cell == (8, 16);
                let (k, n) = (kept.counts(), alone.counts());
                assert_eq!((k.kept, k.uncached), (u64::from(!own), 0), "{o:?} {cell:?}");
                assert_eq!((n.kept, n.uncached), (0, u64::from(!own)), "{o:?} {cell:?}");
            }
        }
    }
}
