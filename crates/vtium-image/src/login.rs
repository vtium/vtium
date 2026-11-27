//! The login picture: what vtium makes of a decoded picture before it
//! places the picture at a login prompt, as a background beneath the text
//! or as a banner above or below the prompt.
//!
//! The picture arrives decoded, in the store's format: premultiplied
//! ARGB, one `u32` a pixel. [`make`] fits it to the form the settings
//! give, scales it once into exactly the pixels the screen shows, and
//! says where the placement goes. Every display path then draws the same
//! stored pixels, so the CPU and GPU paths start from the same bytes.
//!
//! A background is fitted to the display, scaled down to a share of that
//! size, and placed by one of nine anchors against the edges of the
//! character grid. Its placement's box is the cells the picture touches,
//! not the whole grid, so a logo in a corner costs the store only those
//! cells. The pixels of the box that the picture does not reach are
//! transparent, so the picture lands to the pixel at any anchor.
//!
//! The scaler is an area resampler in integer arithmetic. Each pixel it
//! writes is the average of the source pixels under that pixel's area,
//! each weighted by how much of the area it covers. At a scale of one
//! every pixel is copied exactly, so a picture made at the display's own
//! resolution draws one to one. Averaging premultiplied pixels keeps every
//! color within its alpha.
//!
//! References:
//! - ADR-1100 D17: the two forms, the fits, the nine anchors, the scale,
//!   the box of the cells a background covers, the banner's size, position
//!   and limits, the banner at the bottom, and the placement at the lowest
//!   z.

use crate::image::{NewImage, Pixels, Place};
use crate::rules::Protocol;

/// How the picture goes on the screen: one of four fits of a background
/// to the display, or a banner above the login prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Scaled, keeping its shape, until it covers the whole display; what
    /// passes the edges is cut off.
    Cover,
    /// Scaled, keeping its shape, until it fits inside the display; the
    /// bands left over show the default background.
    Contain,
    /// Scaled in width and height apart to fill the display exactly.
    Stretch,
    /// At its own size; what passes the edges is cut off.
    Natural,
    /// A banner at the top of the screen with the login prompt below it,
    /// or at the bottom with the prompt above it.
    Banner,
}

impl Fit {
    /// The fit `login-image-fit`'s word names: `cover`, `contain`,
    /// `stretch`, `none` or `banner`.
    pub fn parse(word: &str) -> Option<Fit> {
        match word {
            "cover" => Some(Fit::Cover),
            "contain" => Some(Fit::Contain),
            "stretch" => Some(Fit::Stretch),
            "none" => Some(Fit::Natural),
            "banner" => Some(Fit::Banner),
            _ => None,
        }
    }
}

/// Where the picture sits: one of nine anchors, named for the edges of the
/// character grid the picture lies against. An anchor that names no
/// column, such as `top`, centers the picture across the screen, and one
/// that names no row, such as `left`, centers it down the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// Where a picture lies along one side of the grid: against its first
/// edge, centered, or against its last edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Start,
    Middle,
    End,
}

impl Position {
    /// The nine anchors, in the order `login-image-position` lists its
    /// words.
    pub const ALL: [Position; 9] = [
        Position::TopLeft,
        Position::Top,
        Position::TopRight,
        Position::Left,
        Position::Center,
        Position::Right,
        Position::BottomLeft,
        Position::Bottom,
        Position::BottomRight,
    ];

    /// The anchor `login-image-position`'s word names, from `top-left` to
    /// `bottom-right`.
    pub fn parse(word: &str) -> Option<Position> {
        Position::ALL.into_iter().find(|p| p.word() == word)
    }

    /// The word `login-image-position` takes for this anchor.
    pub fn word(self) -> &'static str {
        match self {
            Position::TopLeft => "top-left",
            Position::Top => "top",
            Position::TopRight => "top-right",
            Position::Left => "left",
            Position::Center => "center",
            Position::Right => "right",
            Position::BottomLeft => "bottom-left",
            Position::Bottom => "bottom",
            Position::BottomRight => "bottom-right",
        }
    }

    /// Where the anchor puts the picture across the screen.
    fn across(self) -> Side {
        match self {
            Position::TopLeft | Position::Left | Position::BottomLeft => Side::Start,
            Position::Top | Position::Center | Position::Bottom => Side::Middle,
            Position::TopRight | Position::Right | Position::BottomRight => Side::End,
        }
    }

    /// Where the anchor puts the picture down the screen.
    fn down(self) -> Side {
        match self {
            Position::TopLeft | Position::Top | Position::TopRight => Side::Start,
            Position::Left | Position::Center | Position::Right => Side::Middle,
            Position::BottomLeft | Position::Bottom | Position::BottomRight => Side::End,
        }
    }
}

/// `login-image-scale`'s default: the size the fit gives, unchanged.
pub const SCALE_FULL: u8 = 100;

/// The form the settings give the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Form {
    pub fit: Fit,
    /// A banner's height in text rows; `None` for its own size.
    pub rows: Option<u32>,
    /// The anchor `login-image-position` names; `None` when it is unset,
    /// which centers a background and puts a banner at the top left.
    pub position: Option<Position>,
    /// A background's size as a whole percentage of the size its fit
    /// gives, from 1 to [`SCALE_FULL`]. A banner ignores it.
    pub scale: u8,
}

/// The screen the picture is made for: the display's size in pixels, as
/// the text reads on it, the character cell in pixels, the grid in cells,
/// and where the grid starts on the display. The console takes the origin
/// from the renderer's own placement (`vtium_render::grid_origin`), so the
/// picture lines up with the grid whether the renderer centres the grid or
/// puts it flush to the bottom, and on a turned display too.
///
/// References:
/// - ADR-1310 D10 and D12: where the grid sits with smooth scrolling on and
///   off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub display: (u32, u32),
    pub cell: (u32, u32),
    pub grid: (u32, u32),
    /// The pixel, as the text reads, of the grid's top-left corner, from
    /// the display's top-left corner as the text reads.
    pub origin: (u32, u32),
}

/// What the image helper is asked to make of a decoded login picture: the
/// form, and the screen it is made for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fitting {
    pub form: Form,
    pub geometry: Geometry,
}

/// A picture ready to place: its pixels, which every console's store
/// shares, where the placement goes, and for a banner the row the login
/// prompt starts on.
#[derive(Clone, Debug, PartialEq)]
pub struct Made {
    pub width: u32,
    pub height: u32,
    /// Premultiplied ARGB, `width` × `height` of them.
    pub pixels: Pixels,
    pub opaque: bool,
    pub place: Place,
    /// The row, from 0, below a banner; `None` for a background.
    pub prompt_row: Option<u32>,
}

impl Made {
    /// The image the store takes, with its pixels left out for
    /// [`Store::add_shared_image`](crate::Store::add_shared_image) to share:
    /// a kitty picture with no image id and no image number, so no program
    /// can name it.
    pub fn image(&self) -> NewImage {
        NewImage {
            protocol: Protocol::KITTY,
            width: self.width,
            height: self.height,
            pixels: Vec::new(),
            opaque: Some(self.opaque),
            id: 0,
            number: 0,
            transient: false,
        }
    }

    /// The bytes the picture costs a console's store: width × height × 4
    /// and the store's charge for an image.
    pub fn cost(&self) -> u64 {
        u64::from(self.width)
            .saturating_mul(u64::from(self.height))
            .saturating_mul(4)
            .saturating_add(crate::IMAGE_OVERHEAD)
    }
}

/// The lowest z there is. A placement there draws below cell backgrounds
/// and below every picture a program sends.
pub const Z: i32 = i32::MIN;

/// The picture `source`, `size` pixels, made for `form` on `geometry` and
/// placed: [`fit`], [`place`] and [`Spot::made`] together.
pub fn make(form: &Form, geometry: &Geometry, size: (u32, u32), source: &[u32]) -> Option<Made> {
    let fitting = Fitting {
        form: *form,
        geometry: *geometry,
    };
    let (width, height, pixels) = fit(&fitting, size, Source::Argb(source))?;
    let opaque = pixels.iter().all(|p| p >> 24 == 0xff);
    place(&fitting, (width, height))?.made((width, height), pixels, opaque)
}

/// The pixels the screen shows of `source`, a decoded picture of `size`
/// pixels, as `fitting` asks: their width, their height and the pixels.
/// For a background that is the part of the fitted, scaled and anchored
/// picture that falls on the character grid, and for a banner the whole
/// banner. The image helper runs this, so it hands back only these pixels.
/// `None` when the screen has no room for the picture, which [`room`]
/// tells apart: a grid of no cells, a cell of no pixels, or a banner on a
/// grid of 2 rows or fewer.
pub fn fit(
    fitting: &Fitting,
    size: (u32, u32),
    source: Source<'_>,
) -> Option<(u32, u32, Vec<u32>)> {
    let (form, geometry) = (&fitting.form, &fitting.geometry);
    let grid_px = grid_pixels(geometry)?;
    let (w, h) = size;
    if w == 0 || h == 0 {
        return None;
    }
    let len = usize::try_from(u64::from(w).checked_mul(u64::from(h))?).ok()?;
    if source.len() != len {
        return None;
    }
    if form.fit == Fit::Banner {
        let (tw, th) = banner_size(form, geometry, size, grid_px)?;
        let pixels = resample(source, size, (tw, th), (0, 0), (tw, th))?;
        return Some((tw, th, pixels));
    }
    let display = shown_display(geometry, grid_px);
    let origin = grid_origin(geometry, display, grid_px);
    let scaled = shrunk(fitted(form.fit, size, display)?, form.scale);
    let (across, down) = sides(form.position);
    // along each side, how much of the scaled picture shows, and where the
    // picture starts relative to the first pixel of it that shows: 0, or
    // less where the grid cuts it
    let side = |side, d, at, g, s| {
        let v = visible(side, d, at, g, s);
        let window = at.checked_add(i64::from(inset(side, d, at, g, v)?))?;
        Some((v, start(side, d, at, g, s).checked_sub(window)?))
    };
    let (vw, ox) = side(across, display.0, origin.0, grid_px.0, scaled.0)?;
    let (vh, oy) = side(down, display.1, origin.1, grid_px.1, scaled.1)?;
    let pixels = resample(source, size, scaled, (ox, oy), (vw, vh))?;
    Some((vw, vh, pixels))
}

/// Whether the screen `fitting` names has room for a picture in its form:
/// a grid of at least one cell, each cell at least one pixel, and for a
/// banner more than 2 rows, since a banner always leaves the login prompt
/// 2 rows. Where this is false, [`fit`] makes nothing, so the console
/// checks it before it asks the image helper and can log the reason.
pub fn room(fitting: &Fitting) -> bool {
    let geometry = &fitting.geometry;
    if grid_pixels(geometry).is_none() {
        return false;
    }
    let banner_h = geometry
        .grid
        .1
        .checked_sub(2)
        .and_then(|r| r.checked_mul(geometry.cell.1));
    fitting.form.fit != Fit::Banner || banner_h.is_some_and(|h| h > 0)
}

/// Where a picture goes, worked out from its size as [`fit`] made it: its
/// placement, for a banner the row the login prompt starts on, and the
/// box the store holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spot {
    pub place: Place,
    /// The row, from 0, where the login prompt starts: below a banner at
    /// the top, or the first row above a banner at the bottom. `None` for a
    /// background.
    pub prompt_row: Option<u32>,
    /// Where the picture's top left lies in its box, in pixels.
    pub inset: (u32, u32),
    /// The size of the image the store holds, in pixels: the cells of a
    /// background's box, or a banner at its own size.
    pub frame: (u32, u32),
}

impl Spot {
    /// The picture of `size` pixels whose `pixels` [`fit`] made, set in its
    /// box: the pixels of the box outside the picture are transparent, so
    /// the picture lands to the pixel wherever its box starts. `opaque`
    /// says whether every one of `pixels` is opaque. `None` when `pixels`
    /// is not `size` long, or the picture does not lie inside the box.
    pub fn made(self, size: (u32, u32), pixels: Vec<u32>, opaque: bool) -> Option<Made> {
        let padded = self.frame != size;
        let pixels = boxed(pixels, size, self.inset, self.frame)?;
        Some(Made {
            width: self.frame.0,
            height: self.frame.1,
            pixels: Pixels::new(pixels),
            opaque: opaque && !padded,
            place: self.place,
            prompt_row: self.prompt_row,
        })
    }
}

/// Where a picture of `size` pixels, as [`fit`] made it for `fitting`,
/// goes. `None` for a size `fit` does not make for this screen, which the
/// console checks before it reads a pixel of the image helper's answer.
///
/// A background's visible size says where it lies. Against an edge the
/// anchor names, the picture's visible part starts or ends at that edge.
/// Centered on the display, its visible part is centered too when it lies
/// wholly on the grid, and otherwise starts at the grid's first edge, since
/// the spare pixel rows lie before the grid and a centered picture that
/// passes the grid's last edge fills the grid. So the console needs nothing
/// from the image helper but the size.
pub fn place(fitting: &Fitting, size: (u32, u32)) -> Option<Spot> {
    let (form, geometry) = (&fitting.form, &fitting.geometry);
    let grid_px = grid_pixels(geometry)?;
    let (cw, ch) = geometry.cell;
    let (cols, rows) = geometry.grid;
    let (w, h) = size;
    if w == 0 || h == 0 {
        return None;
    }
    if form.fit == Fit::Banner {
        return banner_spot(form, geometry, size, grid_px);
    }
    if w > grid_px.0 || h > grid_px.1 || !full_size_fits(form, size, grid_px) {
        return None;
    }
    let display = shown_display(geometry, grid_px);
    let origin = grid_origin(geometry, display, grid_px);
    let (across, down) = sides(form.position);
    let x = inset(across, display.0, origin.0, grid_px.0, w)?;
    let y = inset(down, display.1, origin.1, grid_px.1, h)?;
    let (col, row) = (x.checked_div(cw)?, y.checked_div(ch)?);
    let box_cols = x.checked_add(w)?.div_ceil(cw).checked_sub(col)?;
    let box_rows = y.checked_add(h)?.div_ceil(ch).checked_sub(row)?;
    if col.checked_add(box_cols)? > cols || row.checked_add(box_rows)? > rows {
        return None;
    }
    Some(Spot {
        place: Place {
            col,
            row,
            cols: box_cols,
            rows: box_rows,
            z: Z,
            ..Place::default()
        },
        prompt_row: None,
        inset: (
            x.checked_sub(col.checked_mul(cw)?)?,
            y.checked_sub(row.checked_mul(ch)?)?,
        ),
        frame: (box_cols.checked_mul(cw)?, box_rows.checked_mul(ch)?),
    })
}

/// Whether a background `size` pixels on the grid can be what its fit
/// makes at a scale of 100: `cover` and `stretch` fill the grid, and
/// `contain` fills it along one side at least. A smaller scale, or `none`,
/// can make any size.
fn full_size_fits(form: &Form, (w, h): (u32, u32), (gw, gh): (u32, u32)) -> bool {
    if form.scale < SCALE_FULL {
        return true;
    }
    match form.fit {
        Fit::Cover | Fit::Stretch => (w, h) == (gw, gh),
        Fit::Contain => w == gw || h == gh,
        Fit::Natural | Fit::Banner => true,
    }
}

/// A banner's spot: at the top, or in the grid's last rows under a bottom
/// anchor, with the anchor's column placing its box.
fn banner_spot(
    form: &Form,
    geometry: &Geometry,
    (w, h): (u32, u32),
    grid_px: (u32, u32),
) -> Option<Spot> {
    let (cw, ch) = geometry.cell;
    let (cols, rows) = geometry.grid;
    let max_h = rows.checked_sub(2)?.checked_mul(ch)?;
    if w > grid_px.0 || h > max_h {
        return None;
    }
    let box_cols = w.div_ceil(cw);
    let box_rows = h.div_ceil(ch);
    let spare = cols.saturating_sub(box_cols);
    // unset, the banner sits at the left margin
    let col = match form.position.map_or(Side::Start, Position::across) {
        Side::Start => 0,
        // an odd number of spare columns leaves the extra one on the right
        Side::Middle => spare / 2,
        Side::End => spare,
    };
    // only a bottom anchor names the bottom: the others mean the top
    let bottom = form.position.is_some_and(|p| p.down() == Side::End);
    let (row, prompt_row) = if bottom {
        (rows.checked_sub(box_rows)?, 0)
    } else {
        (0, box_rows)
    };
    Some(Spot {
        place: Place {
            col,
            row,
            z: Z,
            ..Place::default()
        },
        prompt_row: Some(prompt_row),
        inset: (0, 0),
        frame: (w, h),
    })
}

/// The grid's size in pixels; `None` for a grid of no cells or a cell of
/// no pixels.
fn grid_pixels(geometry: &Geometry) -> Option<(u32, u32)> {
    let (cw, ch) = geometry.cell;
    let (cols, rows) = geometry.grid;
    if cw == 0 || ch == 0 || cols == 0 || rows == 0 {
        return None;
    }
    Some((cols.checked_mul(cw)?, rows.checked_mul(ch)?))
}

/// The display a background is fitted to: the display, at least as large
/// as the grid it holds.
fn shown_display(geometry: &Geometry, (gw, gh): (u32, u32)) -> (u32, u32) {
    let (dw, dh) = geometry.display;
    (dw.max(gw), dh.max(gh))
}

/// Where a background lies across and down the screen: the anchor's
/// column and row, and centered both ways when no anchor is set.
fn sides(position: Option<Position>) -> (Side, Side) {
    position.map_or((Side::Middle, Side::Middle), |p| (p.across(), p.down()))
}

/// Where the grid starts on the `display` that holds it, across and down:
/// the origin `geometry` gives, kept where the whole grid fits on the
/// display.
fn grid_origin(geometry: &Geometry, (dw, dh): (u32, u32), (gw, gh): (u32, u32)) -> (i64, i64) {
    let (x, y) = geometry.origin;
    (
        i64::from(x.min(dw.saturating_sub(gw))),
        i64::from(y.min(dh.saturating_sub(gh))),
    )
}

/// Where a scaled picture `s` long starts on a display side `d` long whose
/// grid side, `g` long, starts at `at`: on the grid's first edge, ending on
/// its last edge, or centered on the display. A centered picture that
/// would end before the grid starts, which only spare pixels longer than
/// the grid allow, starts on the grid's first edge instead, so some of it
/// shows.
fn start(side: Side, d: u32, at: i64, g: u32, s: u32) -> i64 {
    match side {
        Side::Start => at,
        Side::End => at.saturating_add(i64::from(g)).saturating_sub(i64::from(s)),
        Side::Middle => {
            let c = centered(d, s);
            if c.saturating_add(i64::from(s)) <= at {
                at
            } else {
                c
            }
        }
    }
}

/// How much of a scaled picture `s` long, placed as [`start`] places it,
/// lies on the grid side `g` long that starts at `at`.
fn visible(side: Side, d: u32, at: i64, g: u32, s: u32) -> u32 {
    let first = start(side, d, at, g, s);
    let end = first.saturating_add(i64::from(s));
    let shown = end
        .min(at.saturating_add(i64::from(g)))
        .saturating_sub(first.max(at));
    u32::try_from(shown).unwrap_or(0)
}

/// Where the part `v` long that shows of a picture starts on a grid side
/// `g` long that starts at `at` on a display side `d` long, in pixels from
/// the grid's first edge. A centered picture wholly on the grid shows
/// whole, centered on the display. One that passes the grid's first edge,
/// or both its edges, shows from that first edge, where `centered` gives
/// no later start for `v`.
fn inset(side: Side, d: u32, at: i64, g: u32, v: u32) -> Option<u32> {
    match side {
        Side::Start => Some(0),
        Side::End => g.checked_sub(v),
        Side::Middle => u32::try_from(centered(d, v).checked_sub(at)?.max(0)).ok(),
    }
}

/// `size` scaled to `scale` per cent, each side rounded to the nearest
/// pixel and at least 1; a scale past 100 counts as 100, so no scale makes
/// a picture larger than its fit.
fn shrunk((w, h): (u32, u32), scale: u8) -> (u32, u32) {
    let scale = u32::from(scale.min(SCALE_FULL));
    (ratio(w, scale, 100).max(1), ratio(h, scale, 100).max(1))
}

/// `pixels`, a picture of `size`, set at `inset` in a box of `frame`
/// pixels whose other pixels are transparent. The rows move into place in
/// the same buffer, from the last to the first, since each row's place in
/// the box lies at or after its place in the picture.
fn boxed(
    mut pixels: Vec<u32>,
    (w, h): (u32, u32),
    (ix, iy): (u32, u32),
    (bw, bh): (u32, u32),
) -> Option<Vec<u32>> {
    let usize_of = |v: u32| usize::try_from(v).ok();
    let (w, h, ix, iy, bw, bh) = (
        usize_of(w)?,
        usize_of(h)?,
        usize_of(ix)?,
        usize_of(iy)?,
        usize_of(bw)?,
        usize_of(bh)?,
    );
    if pixels.len() != w.checked_mul(h)? || ix.checked_add(w)? > bw || iy.checked_add(h)? > bh {
        return None;
    }
    if (w, h) == (bw, bh) {
        return Some(pixels);
    }
    pixels.resize(bw.checked_mul(bh)?, 0);
    for y in (0..h).rev() {
        let from = y.checked_mul(w)?;
        let row = iy.checked_add(y)?.checked_mul(bw)?;
        let to = row.checked_add(ix)?;
        pixels.copy_within(from..from.checked_add(w)?, to);
        pixels.get_mut(row..to)?.fill(0);
        pixels
            .get_mut(to.checked_add(w)?..row.checked_add(bw)?)?
            .fill(0);
    }
    pixels.get_mut(..iy.checked_mul(bw)?)?.fill(0);
    Some(pixels)
}

/// A banner's size in pixels: its own size or `form.rows` tall, scaled
/// down, keeping its shape, to fit the grid's width and all but two of its
/// rows.
fn banner_size(
    form: &Form,
    geometry: &Geometry,
    (w, h): (u32, u32),
    (max_w, _): (u32, u32),
) -> Option<(u32, u32)> {
    let ch = geometry.cell.1;
    let max_h = geometry.grid.1.checked_sub(2)?.checked_mul(ch)?;
    if max_h == 0 {
        return None;
    }
    let (mut tw, mut th) = match form.rows {
        Some(n) if n > 0 => {
            let th = n.checked_mul(ch)?;
            (ratio(w, th, h).max(1), th)
        }
        _ => (w, h),
    };
    if tw > max_w || th > max_h {
        // width-limited when tw / max_w > th / max_h
        if u64::from(tw).saturating_mul(u64::from(max_h))
            > u64::from(th).saturating_mul(u64::from(max_w))
        {
            th = ratio(th, max_w, tw).max(1);
            tw = max_w;
        } else {
            tw = ratio(tw, max_h, th).max(1);
            th = max_h;
        }
    }
    Some((tw, th))
}

/// `a` × `b` ÷ `c`, rounded to the nearest whole number; 0 when `c` is 0,
/// and `u32::MAX` past it.
fn ratio(a: u32, b: u32, c: u32) -> u32 {
    let n = u64::from(a).saturating_mul(u64::from(b));
    let c = u64::from(c);
    let q = n.saturating_add(c / 2).checked_div(c).unwrap_or(0);
    u32::try_from(q).unwrap_or(u32::MAX)
}

/// The size `size` scales to under `fit` on a display of `display`.
fn fitted(fit: Fit, (w, h): (u32, u32), (dw, dh): (u32, u32)) -> Option<(u32, u32)> {
    // the display is relatively wider than the picture when dw/dh >= w/h
    let wider =
        u64::from(dw).saturating_mul(u64::from(h)) >= u64::from(dh).saturating_mul(u64::from(w));
    let by_width = (dw, ratio(h, dw, w).max(1));
    let by_height = (ratio(w, dh, h).max(1), dh);
    match fit {
        Fit::Cover if wider => Some(by_width),
        Fit::Cover => Some(by_height),
        Fit::Contain if wider => Some(by_height),
        Fit::Contain => Some(by_width),
        Fit::Stretch => Some((dw, dh)),
        Fit::Natural => Some((w, h)),
        Fit::Banner => None,
    }
}

/// Where a picture `scaled` long starts on a side `side` long when it is
/// centered: negative when it passes both edges. The spare pixels are
/// split with the extra one after the picture.
fn centered(side: u32, scaled: u32) -> i64 {
    (i64::from(side).saturating_sub(i64::from(scaled))).div_euclid(2)
}

/// The total of each pixel's weights, 2^16.
const ONE: u32 = 1 << 16;

/// The source pixels under one output pixel along one side: the first,
/// and where its weights start in [`Taps::weights`] and how many there are.
#[derive(Clone, Copy, Debug)]
struct Tap {
    first: usize,
    at: usize,
    len: usize,
}

/// One side's taps: for each output pixel, the source pixels it averages,
/// or `None` where the output pixel lies outside the scaled picture.
struct Taps {
    taps: Vec<Option<Tap>>,
    weights: Vec<u32>,
}

/// The taps of a side `src` long scaled to `scaled`, for `out` output
/// pixels, the first at `offset` on the display while the scaled picture
/// starts at 0. Each tap's weights add up to [`ONE`].
fn taps(src: u32, scaled: u32, offset: i64, out: u32) -> Option<Taps> {
    let (s, d) = (u64::from(src), u64::from(scaled));
    if s == 0 || d == 0 {
        return None;
    }
    let mut taps = Vec::with_capacity(usize::try_from(out).ok()?);
    let mut weights = Vec::new();
    for i in 0..out {
        // the output pixel's place in the scaled picture
        let u = i64::from(i).checked_sub(offset)?;
        let Some(u) = u64::try_from(u).ok().filter(|&u| u < d) else {
            taps.push(None);
            continue;
        };
        // its area, in units of 1/scaled of a source pixel
        let (lo, hi) = (u.checked_mul(s)?, u.checked_add(1)?.checked_mul(s)?);
        let (first, last) = (lo.checked_div(d)?, hi.checked_sub(1)?.checked_div(d)?);
        let at = weights.len();
        let mut total = 0u32;
        let mut largest = (at, 0u32);
        for k in first..=last.min(s.saturating_sub(1)) {
            let start = k.checked_mul(d)?.max(lo);
            let end = k.checked_add(1)?.checked_mul(d)?.min(hi);
            let overlap = end.saturating_sub(start);
            let w = u32::try_from(overlap.checked_mul(u64::from(ONE))?.checked_div(s)?).ok()?;
            if w > largest.1 {
                largest = (weights.len(), w);
            }
            total = total.saturating_add(w);
            weights.push(w);
        }
        // the rounding left over goes to the heaviest source pixel
        if let Some(w) = weights.get_mut(largest.0) {
            *w = w.saturating_add(ONE.saturating_sub(total));
        }
        let len = weights.len().checked_sub(at)?;
        taps.push(Some(Tap {
            first: usize::try_from(first).ok()?,
            at,
            len,
        }));
    }
    Some(Taps { taps, weights })
}

impl Taps {
    /// The source pixels and weights of output pixel `i`.
    fn of(&self, i: usize) -> Option<impl Iterator<Item = (usize, u32)> + '_> {
        let tap = (*self.taps.get(i)?)?;
        let end = tap.at.checked_add(tap.len)?;
        let weights = self.weights.get(tap.at..end)?;
        Some(
            weights
                .iter()
                .enumerate()
                .map(move |(n, &w)| (tap.first.saturating_add(n), w)),
        )
    }
}

/// A decoded picture's pixels, as the store keeps them or as the image
/// helper decodes them.
#[derive(Clone, Copy)]
pub enum Source<'a> {
    /// Premultiplied ARGB, one `u32` a pixel.
    Argb(&'a [u32]),
    /// Premultiplied R, G, B, A bytes, four a pixel: the helper's decoders'
    /// output, read in place so the helper keeps no second copy.
    Rgba(&'a [u8]),
}

impl Source<'_> {
    fn len(self) -> usize {
        match self {
            Source::Argb(p) => p.len(),
            Source::Rgba(b) => b.len().checked_div(4).unwrap_or(0),
        }
    }

    /// Pixel `i` as premultiplied ARGB; 0 past the end.
    fn get(self, i: usize) -> u32 {
        match self {
            Source::Argb(p) => p.get(i).copied().unwrap_or(0),
            Source::Rgba(b) => i
                .checked_mul(4)
                .and_then(|at| b.get(at..at.checked_add(4)?))
                .and_then(|px| <[u8; 4]>::try_from(px).ok())
                .map_or(0, |px| {
                    let [r, g, b, a] = px.map(u32::from);
                    a << 24 | r << 16 | g << 8 | b
                }),
        }
    }
}

/// A source row resampled across: each output pixel's four channels,
/// A, R, G, B, each times [`ONE`].
type Row = Vec<[u32; 4]>;

/// The channels of `p`, A, R, G, B.
fn channels(p: u32) -> [u32; 4] {
    [p >> 24, (p >> 16) & 0xff, (p >> 8) & 0xff, p & 0xff]
}

/// `source`, `size` pixels, scaled to `scaled` and read through a window
/// of `out` pixels whose top left lies at `offset` in display terms, where
/// the scaled picture starts at 0. Pixels of the window outside the scaled
/// picture are transparent. `None` when a size overflows.
fn resample(
    source: Source<'_>,
    (w, h): (u32, u32),
    scaled: (u32, u32),
    offset: (i64, i64),
    (ow, oh): (u32, u32),
) -> Option<Vec<u32>> {
    // the output's pixel i sits at display i; in the scaled picture that
    // is i - offset
    let across = taps(w, scaled.0, offset.0, ow)?;
    let down = taps(h, scaled.1, offset.1, oh)?;
    let width = usize::try_from(w).ok()?;
    let out_w = usize::try_from(ow).ok()?;
    let len = out_w.checked_mul(usize::try_from(oh).ok()?)?;
    let mut out = Vec::with_capacity(len);
    // the rows resampled across, newest last: an output row shares at
    // most its first source row with the row above it
    let mut kept: Vec<(usize, Row)> = Vec::new();
    for j in 0..usize::try_from(oh).ok()? {
        let Some(rows) = down.of(j) else {
            out.extend(std::iter::repeat_n(0, out_w));
            continue;
        };
        let mut acc = vec![[0u64; 4]; out_w];
        for (k, wy) in rows {
            if !kept.iter().any(|(r, _)| *r == k) {
                let start = k.checked_mul(width)?;
                if start.checked_add(width)? > source.len() {
                    return None;
                }
                let row = across_row(source, start, &across, out_w);
                kept.retain(|(r, _)| r.saturating_add(1) >= k);
                kept.push((k, row));
            }
            let row = &kept.iter().find(|(r, _)| *r == k)?.1;
            for (a, px) in acc.iter_mut().zip(row) {
                for (c, v) in a.iter_mut().zip(px) {
                    *c = c.saturating_add(u64::from(wy).saturating_mul(u64::from(*v)));
                }
            }
        }
        for (i, a) in acc.iter().enumerate() {
            if across.taps.get(i).is_none_or(Option::is_none) {
                out.push(0);
                continue;
            }
            // each channel times 2^32, rounded to the nearest
            let [al, r, g, b] = a.map(|c| {
                let v = c.saturating_add(1 << 31) >> 32;
                u32::try_from(v.min(0xff)).unwrap_or(0xff)
            });
            out.push(al << 24 | r.min(al) << 16 | g.min(al) << 8 | b.min(al));
        }
    }
    Some(out)
}

/// The source row that starts at pixel `start`, resampled across through
/// `across`.
fn across_row(source: Source<'_>, start: usize, across: &Taps, out_w: usize) -> Row {
    (0..out_w)
        .map(|i| {
            let mut px = [0u32; 4];
            if let Some(cols) = across.of(i) {
                for (k, wx) in cols {
                    let ch = channels(source.get(start.saturating_add(k)));
                    for (c, v) in px.iter_mut().zip(ch) {
                        *c = c.saturating_add(wx.saturating_mul(v));
                    }
                }
            }
            px
        })
        .collect()
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "a test fails by panicking, and works out its pixels in small numbers"
)]
mod tests;
