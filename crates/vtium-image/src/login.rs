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
/// the text reads on it, the character cell in pixels, and the grid in
/// cells. The renderer centers the grid across the display, with the odd
/// spare pixel after it, and puts the grid's last row on the display's
/// bottom edge, so the spare pixel rows lie above the grid.
///
/// References:
/// - ADR-1310 D10: the grid flush to the text's bottom edge, and centered
///   across.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub display: (u32, u32),
    pub cell: (u32, u32),
    pub grid: (u32, u32),
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
    let origin = grid_origin(display, grid_px);
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
    let origin = grid_origin(display, grid_px);
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

/// Where the grid starts on the `display` that holds it, across and down,
/// as the renderer places it: centered across, with the odd spare pixel
/// after it, and its last row on the display's bottom edge.
///
/// References:
/// - ADR-1310 D10: the grid flush to the text's bottom edge, and centered
///   across.
fn grid_origin((dw, dh): (u32, u32), (gw, gh): (u32, u32)) -> (i64, i64) {
    (
        centered(dw, gw),
        i64::from(dh).saturating_sub(i64::from(gh)),
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
mod tests {
    use super::*;

    /// A picture whose every pixel is opaque and differs from its
    /// neighbours.
    fn picture(w: u32, h: u32) -> Vec<u32> {
        (0..h)
            .flat_map(|y| {
                (0..w).map(move |x| 0xff00_0000 | (x & 0xff) << 16 | (y & 0xff) << 8 | 0x40)
            })
            .collect()
    }

    fn form(fit: Fit) -> Form {
        Form {
            fit,
            rows: None,
            position: None,
            scale: SCALE_FULL,
        }
    }

    const SCREEN: Geometry = Geometry {
        display: (80, 48),
        cell: (8, 16),
        grid: (10, 3),
    };

    #[test]
    fn a_background_made_at_the_displays_size_draws_one_to_one_under_cover() {
        let src = picture(80, 48);
        let made = make(&form(Fit::Cover), &SCREEN, (80, 48), &src).expect("made");
        assert_eq!((made.width, made.height), (80, 48));
        assert_eq!(made.pixels.as_slice(), &src[..]);
        assert!(made.opaque);
        assert_eq!(
            (made.place.cols, made.place.rows, made.place.z),
            (10, 3, i32::MIN)
        );
        assert_eq!(made.prompt_row, None);
    }

    #[test]
    fn a_background_keeps_the_part_that_falls_on_the_grid() {
        // a display of 84x50 with a grid of 80x48 on it, centered across
        // with 2 pixel columns before it, and its last row on the display's
        // bottom edge with the 2 spare pixel rows above it: the picture of
        // 84x50 at one to one, cut to the grid, so the grid's first row
        // shows the picture's third
        let src = picture(84, 50);
        let geo = Geometry {
            display: (84, 50),
            ..SCREEN
        };
        let made = make(&form(Fit::Cover), &geo, (84, 50), &src).expect("made");
        assert_eq!((made.width, made.height), (80, 48));
        for y in 0..48 {
            for x in 0..80 {
                assert_eq!(
                    made.pixels.as_slice()[y * 80 + x],
                    src[(y + 2) * 84 + x + 2]
                );
            }
        }
    }

    #[test]
    fn contain_leaves_transparent_bands_and_cover_crops() {
        // a square picture on a display twice as wide as tall
        let src = vec![0xffff_0000; 16 * 16];
        let made = make(&form(Fit::Contain), &SCREEN, (16, 16), &src).expect("made");
        // 48 tall, so 48 wide, centered: 16 transparent columns each side,
        // outside the box of columns 2 to 7
        let at = |x| shown(&made, SCREEN.cell, (x, 0));
        assert_eq!((at(0), at(15)), (0, 0));
        assert_eq!((at(16), at(63)), (0xffff_0000, 0xffff_0000));
        assert_eq!((at(64), at(79)), (0, 0));
        assert_eq!((made.place.col, made.place.cols), (2, 6));
        let made = make(&form(Fit::Cover), &SCREEN, (16, 16), &src).expect("made");
        assert!(made.opaque);
        assert_eq!((made.place.cols, made.place.rows), (10, 3));
    }

    #[test]
    fn stretch_fills_and_none_centres_at_its_own_size() {
        let src = vec![0xff00_ff00; 4 * 4];
        let made = make(&form(Fit::Stretch), &SCREEN, (4, 4), &src).expect("made");
        assert!(made.opaque);
        let made = make(&form(Fit::Natural), &SCREEN, (4, 4), &src).expect("made");
        // 4x4 centered on 80x48: columns 38 to 41, rows 22 to 25
        let at = |x, y| shown(&made, SCREEN.cell, (x, y));
        assert_eq!(at(37, 22), 0);
        assert_eq!(at(38, 22), 0xff00_ff00);
        assert_eq!(at(41, 25), 0xff00_ff00);
        assert_eq!(at(42, 25), 0);
        assert_eq!(at(38, 26), 0);
    }

    #[test]
    fn downscaling_averages_and_keeps_colors_within_alpha() {
        // black and half-transparent white, alternating: each 2x2 square
        // averages to the same pixel
        let src: Vec<u32> = (0..16)
            .map(|i| {
                if (i + i / 4) % 2 == 0 {
                    0xff00_0000
                } else {
                    0x8080_8080
                }
            })
            .collect();
        let out = resample(Source::Argb(&src), (4, 4), (2, 2), (0, 0), (2, 2)).expect("resampled");
        // alpha (255 + 128) / 2 = 191.5, color 64
        assert_eq!(out, vec![0xc040_4040; 4]);
        for p in out {
            let [a, r, g, b] = channels(p);
            assert!(r <= a && g <= a && b <= a);
        }
    }

    #[test]
    fn a_banner_takes_its_own_size_and_the_prompt_starts_below_it() {
        let src = picture(20, 20);
        let geo = Geometry {
            grid: (10, 5),
            ..SCREEN
        };
        let made = make(&form(Fit::Banner), &geo, (20, 20), &src).expect("made");
        assert_eq!((made.width, made.height), (20, 20));
        assert_eq!(made.pixels.as_slice(), &src[..]);
        // 20 pixels of 16-pixel rows: 2 rows
        assert_eq!(made.prompt_row, Some(2));
        assert_eq!(
            (made.place.col, made.place.cols, made.place.rows),
            (0, 0, 0)
        );
    }

    #[test]
    fn a_banner_of_n_rows_and_one_too_large_for_the_grid() {
        let src = picture(40, 20);
        let geo = Geometry {
            grid: (80, 10),
            ..SCREEN
        };
        let rows = Form {
            rows: Some(3),
            ..form(Fit::Banner)
        };
        let made = make(&rows, &geo, (40, 20), &src).expect("made");
        assert_eq!((made.width, made.height), (96, 48));
        assert_eq!(made.prompt_row, Some(3));
        // 20 rows asked of a grid of 10: scaled down to 8 rows, 128 pixels
        let tall = Form {
            rows: Some(20),
            ..form(Fit::Banner)
        };
        let made = make(&tall, &geo, (40, 20), &src).expect("made");
        assert_eq!((made.width, made.height), (256, 128));
        assert_eq!(made.prompt_row, Some(8));
        // a grid of 2 rows has no room for a banner
        let flat = Geometry {
            grid: (80, 2),
            ..SCREEN
        };
        assert_eq!(make(&form(Fit::Banner), &flat, (40, 20), &src), None);
    }

    #[test]
    fn a_centred_banner_leaves_the_odd_column_on_its_right() {
        // 9 columns of 8 pixels on a grid of 80: 71 spare, 35 on the left
        let src = picture(72, 16);
        let geo = Geometry {
            display: (640, 160),
            cell: (8, 16),
            grid: (80, 10),
        };
        let at = |position| {
            let f = Form {
                position: Some(position),
                ..form(Fit::Banner)
            };
            make(&f, &geo, (72, 16), &src).expect("made").place.col
        };
        assert_eq!(at(Position::Left), 0);
        assert_eq!(at(Position::Center), 35);
        assert_eq!(at(Position::Right), 71);
    }

    #[test]
    fn a_picture_of_the_wrong_length_or_no_room_is_refused() {
        assert_eq!(make(&form(Fit::Cover), &SCREEN, (4, 4), &[0; 15]), None);
        let none = Geometry {
            grid: (0, 3),
            ..SCREEN
        };
        assert_eq!(make(&form(Fit::Cover), &none, (4, 4), &[0; 16]), None);
    }

    // The console checks the image helper's answer before it reads a
    // pixel: at a scale of 100, cover and stretch fill the grid and contain
    // fills one side of it, so a picture the helper did not fit is refused.
    #[test]
    fn a_size_the_fit_does_not_make_is_refused_before_a_pixel_is_read() {
        let fitting = |fit, scale| Fitting {
            form: Form { scale, ..form(fit) },
            geometry: SCREEN,
        };
        for fit in [Fit::Cover, Fit::Stretch, Fit::Contain] {
            assert_eq!(place(&fitting(fit, SCALE_FULL), (8, 8)), None, "{fit:?}");
            assert!(place(&fitting(fit, SCALE_FULL), (80, 48)).is_some());
            assert!(place(&fitting(fit, 50), (8, 8)).is_some(), "{fit:?}");
        }
        assert!(place(&fitting(Fit::Contain, SCALE_FULL), (40, 48)).is_some());
        assert!(place(&fitting(Fit::Natural, SCALE_FULL), (8, 8)).is_some());
        // nothing larger than the grid, and nothing empty
        assert_eq!(place(&fitting(Fit::Natural, SCALE_FULL), (81, 8)), None);
        assert_eq!(place(&fitting(Fit::Natural, SCALE_FULL), (0, 8)), None);
    }

    /// A small pseudo-random number generator, xorshift64, so a sweep is
    /// the same on every run.
    struct Dice(u64);

    impl Dice {
        /// A number from `lo` to `hi`, both included.
        fn roll(&mut self, lo: u32, hi: u32) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            lo + (self.0 % u64::from(hi - lo + 1)) as u32
        }
    }

    // The image helper runs `fit`, and the console checks the helper's
    // answer with `place` and `Spot::made`. Over random screens, forms and
    // sizes, nothing panics, `room` says exactly when `fit` makes a
    // picture, every size `fit` makes passes the console's checks, and
    // `place` refuses any other size or gives a box that holds it.
    #[test]
    fn fit_place_and_made_agree_over_random_screens_and_forms() {
        let fits = [
            Fit::Contain,
            Fit::Cover,
            Fit::Stretch,
            Fit::Natural,
            Fit::Banner,
        ];
        let mut dice = Dice(0x9e37_79b9_7f4a_7c15);
        for case in 0..4000 {
            let cell = (dice.roll(0, 12), dice.roll(0, 20));
            let grid = (dice.roll(0, 24), dice.roll(0, 12));
            let spare = (dice.roll(0, 12), dice.roll(0, 12));
            let geometry = Geometry {
                display: (grid.0 * cell.0 + spare.0, grid.1 * cell.1 + spare.1),
                cell,
                grid,
            };
            let position = match dice.roll(0, 9) {
                9 => None,
                n => Position::ALL.get(n as usize).copied(),
            };
            let form = Form {
                fit: fits[dice.roll(0, 4) as usize],
                rows: [None, Some(dice.roll(0, 6))][dice.roll(0, 1) as usize],
                position,
                scale: [SCALE_FULL, dice.roll(1, 100) as u8][dice.roll(0, 1) as usize],
            };
            let fitting = Fitting { form, geometry };
            let size = (dice.roll(1, 64), dice.roll(1, 64));
            let src = picture(size.0, size.1);
            let made = fit(&fitting, size, Source::Argb(&src));
            assert_eq!(made.is_some(), room(&fitting), "case {case}: {fitting:?}");
            if let Some((w, h, pixels)) = made {
                let spot = place(&fitting, (w, h));
                assert!(spot.is_some(), "case {case}: {fitting:?} refuses {w}×{h}");
                let made = spot.and_then(|spot| spot.made((w, h), pixels, true));
                assert!(made.is_some(), "case {case}: {fitting:?}");
            }
            let other = (dice.roll(0, 300), dice.roll(0, 300));
            if let Some(spot) = place(&fitting, other) {
                assert!(
                    spot.inset.0 + other.0 <= spot.frame.0
                        && spot.inset.1 + other.1 <= spot.frame.1,
                    "case {case}: {fitting:?} puts {other:?} outside its box"
                );
                let pixels = vec![0xff00_0000; (other.0 * other.1) as usize];
                assert!(spot.made(other, pixels, true).is_some(), "case {case}");
            }
        }
    }

    // A screen has room for a background when its grid has a cell, and
    // for a banner when it also has more than 2 rows.
    #[test]
    fn room_needs_a_cell_and_a_banner_needs_three_rows() {
        let fitting = |fit, grid, cell| Fitting {
            form: form(fit),
            geometry: Geometry {
                grid,
                cell,
                ..SCREEN
            },
        };
        assert!(room(&fitting(Fit::Cover, (10, 3), (8, 16))));
        assert!(room(&fitting(Fit::Banner, (10, 3), (8, 16))));
        assert!(room(&fitting(Fit::Cover, (10, 2), (8, 16))));
        assert!(!room(&fitting(Fit::Banner, (10, 2), (8, 16))));
        assert!(!room(&fitting(Fit::Cover, (0, 3), (8, 16))));
        assert!(!room(&fitting(Fit::Contain, (10, 3), (8, 0))));
    }

    #[test]
    fn the_words_of_the_keys() {
        assert_eq!(Fit::parse("none"), Some(Fit::Natural));
        assert_eq!(Fit::parse("banner"), Some(Fit::Banner));
        assert_eq!(Fit::parse("fill"), None);
        assert_eq!(Position::parse("center"), Some(Position::Center));
        for p in Position::ALL {
            assert_eq!(Position::parse(p.word()), Some(p));
        }
        for word in ["middle", "top-center", "30%", "Top", ""] {
            assert_eq!(Position::parse(word), None, "{word:?}");
        }
    }

    /// The form of a background under `fit` at `position` and `scale`.
    fn placed(fit: Fit, position: Option<Position>, scale: u8) -> Form {
        Form {
            fit,
            rows: None,
            position,
            scale,
        }
    }

    /// The pixel of `made` at `(x, y)` on the grid, in pixels from the
    /// grid's top left; `None` outside its box.
    fn on_grid(made: &Made, cell: (u32, u32), (x, y): (u32, u32)) -> Option<u32> {
        let x0 = made.place.col * cell.0;
        let y0 = made.place.row * cell.1;
        let (bx, by) = (x.checked_sub(x0)?, y.checked_sub(y0)?);
        if bx >= made.width || by >= made.height {
            return None;
        }
        made.pixels
            .as_slice()
            .get(usize::try_from(by * made.width + bx).ok()?)
            .copied()
    }

    /// The pixel of `made` the grid shows at `(x, y)`: transparent outside
    /// its box.
    fn shown(made: &Made, cell: (u32, u32), at: (u32, u32)) -> u32 {
        on_grid(made, cell, at).unwrap_or(0)
    }

    /// The opaque pixels of `made` as a rectangle on the grid: its left,
    /// top, width and height in pixels.
    fn picture_on_grid(made: &Made, cell: (u32, u32)) -> (u32, u32, u32, u32) {
        let w = made.width;
        let opaque: Vec<(u32, u32)> = (0..made.height)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| made.pixels.as_slice()[(y * w + x) as usize] >> 24 != 0)
            .collect();
        let left = opaque.iter().map(|p| p.0).min().expect("a pixel");
        let right = opaque.iter().map(|p| p.0).max().expect("a pixel");
        let top = opaque.iter().map(|p| p.1).min().expect("a pixel");
        let bottom = opaque.iter().map(|p| p.1).max().expect("a pixel");
        // the opaque pixels fill their rectangle
        assert_eq!(
            opaque.len() as u32,
            (right - left + 1) * (bottom - top + 1),
            "a picture with a hole"
        );
        (
            made.place.col * cell.0 + left,
            made.place.row * cell.1 + top,
            right - left + 1,
            bottom - top + 1,
        )
    }

    /// A picture whose pixel at `(x, y)` holds `x` in its red byte and `y`
    /// in its green byte, opaque.
    fn coordinates(w: u32, h: u32) -> Vec<u32> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| 0xff00_0000 | x << 16 | y << 8))
            .collect()
    }

    // A display of 100×60 pixels with a grid of 12×5 cells of 7×10 pixels
    // on it, centered across and its last row on the display's bottom
    // edge: the grid starts 8 pixels in and 10 pixels down.
    const ROOMY: Geometry = Geometry {
        display: (100, 60),
        cell: (7, 10),
        grid: (12, 5),
    };

    // A picture of 15×30 under contain on the display of 100×60 scales
    // twice over to 30×60, which copies each source pixel to 2×2. It is 10
    // pixels taller than the grid, so the anchor's row chooses which 50
    // rows show, and it leaves 54 pixels of room across, which the
    // anchor's column places it in.
    //
    // References:
    // - ADR-1100 D17, "Where the picture sits": how each anchor places a
    //   background.
    #[test]
    fn each_anchor_places_a_contained_picture_at_the_offset_worked_out_by_hand() {
        let src = coordinates(15, 30);
        for position in Position::ALL {
            let made = make(
                &placed(Fit::Contain, Some(position), SCALE_FULL),
                &ROOMY,
                (15, 30),
                &src,
            )
            .expect("made");
            // across: against the grid's left edge; centered on the
            // display, (100 - 30) / 2 = 35 less the grid's 8; against the
            // grid's right edge, 84 - 30
            let x = match position {
                Position::TopLeft | Position::Left | Position::BottomLeft => 0,
                Position::Top | Position::Center | Position::Bottom => 27,
                _ => 54,
            };
            // down: the first scaled row that shows, 0 at the top; centered,
            // the picture starts at the display's top, 10 rows above the
            // grid, and at the bottom its last row is the grid's last, so
            // 10 rows of it lie above the grid too
            let first_row = match position {
                Position::TopLeft | Position::Top | Position::TopRight => 0,
                _ => 10,
            };
            let at = picture_on_grid(&made, ROOMY.cell);
            assert_eq!(at, (x, 0, 30, 50), "{position:?}");
            let corner = on_grid(&made, ROOMY.cell, (x, 0)).expect("the corner");
            assert_eq!(
                corner,
                0xff00_0000 | (first_row / 2) << 8,
                "{position:?}: the top left shows source row {}",
                first_row / 2
            );
        }
    }

    // A picture shorter than the grid, centered on the display, can reach
    // into the spare pixel rows above the grid, and the grid shows only the
    // rest of it. At its own size, 20×46 on ROOMY's display of 100×60, it
    // spans the display's rows 7 to 52, and the grid starts at row 10, so
    // the grid's first row shows the picture's row 3 and its last 43 rows
    // show. Across, it starts at (100 - 20) / 2 = 40, the grid's 32.
    //
    // References:
    // - ADR-1100 D17, "Where the picture sits": a side the anchor does not
    //   name centers the picture on the display.
    // - ADR-1310 D10: the grid's last row on the display's bottom edge.
    #[test]
    fn a_centered_picture_shorter_than_the_grid_loses_its_rows_above_the_grid() {
        let src = coordinates(20, 46);
        let form = placed(Fit::Natural, None, SCALE_FULL);
        // the image helper hands back only the rows that show
        let fitting = Fitting {
            form,
            geometry: ROOMY,
        };
        let (w, h, _) = fit(&fitting, (20, 46), Source::Argb(&src)).expect("fitted");
        assert_eq!((w, h), (20, 43));
        let made = make(&form, &ROOMY, (20, 46), &src).expect("made");
        assert_eq!(picture_on_grid(&made, ROOMY.cell), (32, 0, 20, 43));
        assert_eq!(
            on_grid(&made, ROOMY.cell, (32, 0)),
            Some(0xff00_0000 | 3 << 8)
        );
        assert_eq!(
            on_grid(&made, ROOMY.cell, (51, 42)),
            Some(0xff00_0000 | 19 << 16 | 45 << 8)
        );
    }

    // Unset, a background is centered, which is where `center` puts it.
    #[test]
    fn an_unset_anchor_centers_a_background() {
        let src = coordinates(15, 30);
        let make_at = |position| {
            make(
                &placed(Fit::Contain, position, SCALE_FULL),
                &ROOMY,
                (15, 30),
                &src,
            )
            .expect("made")
        };
        assert_eq!(make_at(None), make_at(Some(Position::Center)));
    }

    // Under cover at `top`, the picture's top edge lies on the grid's top
    // edge, so the source's top row shows on the grid's first pixel row and
    // the bottom is cut.
    #[test]
    fn cover_at_top_keeps_the_sources_top_row() {
        // a square, red in its top row and blue below: cover scales it to
        // 100×100, so its first 6 or 7 rows come from the red row alone
        let src: Vec<u32> = (0..15 * 15)
            .map(|i| if i < 15 { 0xffff_0000 } else { 0xff00_00ff })
            .collect();
        let top = make(
            &placed(Fit::Cover, Some(Position::Top), SCALE_FULL),
            &ROOMY,
            (15, 15),
            &src,
        )
        .expect("made");
        for x in 0..84 {
            assert_eq!(on_grid(&top, ROOMY.cell, (x, 0)), Some(0xffff_0000), "{x}");
        }
        let center = make(
            &placed(Fit::Cover, Some(Position::Center), SCALE_FULL),
            &ROOMY,
            (15, 15),
            &src,
        )
        .expect("made");
        assert_eq!(on_grid(&center, ROOMY.cell, (0, 0)), Some(0xff00_00ff));
    }

    // A logo of 1000×1000 on a display of 1920×1080 with 8×16 cells,
    // contained at 1080×1080 and scaled to 30%, at `bottom-right`.
    //
    // References:
    // - ADR-1100 D17, "A background at a share of its size": this example,
    //   with its size and its box.
    #[test]
    fn a_scaled_logo_in_a_corner_has_the_size_and_box_worked_out_by_hand() {
        let hd = Geometry {
            display: (1920, 1080),
            cell: (8, 16),
            grid: (240, 67),
        };
        let src = vec![0xff40_8020; 1000 * 1000];
        let made = make(
            &placed(Fit::Contain, Some(Position::BottomRight), 30),
            &hd,
            (1000, 1000),
            &src,
        )
        .expect("made");
        // 324×324, its right and bottom edges on the grid's, 1920 and 1072
        assert_eq!(picture_on_grid(&made, hd.cell), (1596, 748, 324, 324));
        // the box: columns 199 to 239 and rows 46 to 66
        assert_eq!(
            (
                made.place.col,
                made.place.row,
                made.place.cols,
                made.place.rows
            ),
            (199, 46, 41, 21)
        );
        assert_eq!((made.width, made.height), (328, 336));
        assert_eq!(made.cost(), 441_344);
        assert!(!made.opaque);
        // under none at 50, a photo of 3840×2160 draws at 1920×1080: on the
        // grid of 1920×1072, centered, that is the whole grid
        let photo = vec![0xff10_2030; 3840 * 2160];
        let made = make(&placed(Fit::Natural, None, 50), &hd, (3840, 2160), &photo).expect("made");
        assert_eq!(picture_on_grid(&made, hd.cell), (0, 0, 1920, 1072));
    }

    // No scale makes a picture larger than its fit, and each side keeps at
    // least one pixel.
    #[test]
    fn no_scale_makes_a_picture_larger_than_its_fit() {
        for fit in [Fit::Contain, Fit::Cover, Fit::Stretch, Fit::Natural] {
            for size in [(1, 1), (3, 7), (1000, 10), (97, 61)] {
                let full = fitted(fit, size, ROOMY.display).expect("fitted");
                for scale in 1..=100 {
                    let (w, h) = shrunk(full, scale);
                    assert!(w <= full.0 && h <= full.1, "{fit:?} {size:?} at {scale}");
                    assert!(w >= 1 && h >= 1, "{fit:?} {size:?} at {scale}");
                }
                assert_eq!(shrunk(full, 100), full);
                // a scale past 100 counts as 100
                assert_eq!(shrunk(full, 101), full);
                assert_eq!(shrunk(full, 255), full);
            }
        }
        // 30% of 1080, rounded to the nearest pixel, and 1% of 3 pixels
        assert_eq!(shrunk((1080, 1080), 30), (324, 324));
        assert_eq!(shrunk((3, 150), 1), (1, 2));
    }

    // The box is the cells the picture touches: a picture of 5×3 pixels
    // under none on cells of 4×4, at the grid's right edge, starts in the
    // middle of a cell, so its box is the two cells it touches across and
    // the one down, and the pixels of the box outside it are transparent.
    #[test]
    fn the_box_is_the_cells_the_picture_touches_and_clear_outside_it() {
        let geo = Geometry {
            display: (40, 20),
            cell: (4, 4),
            grid: (10, 5),
        };
        let src = vec![0xffff_ffff; 5 * 3];
        for position in Position::ALL {
            let made = make(
                &placed(Fit::Natural, Some(position), SCALE_FULL),
                &geo,
                (5, 3),
                &src,
            )
            .expect("made");
            let (x, y, w, h) = picture_on_grid(&made, geo.cell);
            assert_eq!((w, h), (5, 3), "{position:?}");
            let cols = (x + w).div_ceil(4) - x / 4;
            let rows = (y + h).div_ceil(4) - y / 4;
            assert_eq!(
                (
                    made.place.col,
                    made.place.row,
                    made.place.cols,
                    made.place.rows
                ),
                (x / 4, y / 4, cols, rows),
                "{position:?}"
            );
            assert_eq!((made.width, made.height), (cols * 4, rows * 4));
            // every pixel outside the picture is transparent
            let clear = made.pixels.as_slice().iter().filter(|&&p| p == 0).count();
            assert_eq!(clear as u32, cols * rows * 16 - 15, "{position:?}");
        }
    }

    /// A banner of 20×20 pixels on a grid of 10×6 cells of 8×16 at
    /// `position`: its placement and the row the login prompt starts on.
    fn banner_at(position: Option<Position>, size: (u32, u32)) -> (Place, Option<u32>) {
        let geo = Geometry {
            display: (80, 96),
            cell: (8, 16),
            grid: (10, 6),
        };
        let f = Form {
            position,
            ..form(Fit::Banner)
        };
        let made = make(&f, &geo, size, &picture(size.0, size.1)).expect("made");
        (made.place, made.prompt_row)
    }

    // A bottom anchor puts the banner's box in the grid's last rows, and
    // the login prompt starts on the first row.
    //
    // References:
    // - ADR-1100 D17, "A banner at the bottom": where the banner and the
    //   prompt go.
    #[test]
    fn a_bottom_banner_takes_the_last_rows_and_the_prompt_starts_at_the_top() {
        // 20 pixels of 16-pixel rows: 2 rows, rows 4 and 5 of 6; 3 columns
        let (place, prompt) = banner_at(Some(Position::BottomRight), (20, 20));
        assert_eq!((place.col, place.row, place.z), (7, 4, Z));
        assert_eq!(prompt, Some(0));
        let (place, _) = banner_at(Some(Position::BottomLeft), (20, 20));
        assert_eq!((place.col, place.row), (0, 4));
        let (place, _) = banner_at(Some(Position::Bottom), (20, 20));
        assert_eq!((place.col, place.row), (3, 4));
        // a banner taller than the grid keeps two rows free above it
        let (place, prompt) = banner_at(Some(Position::Bottom), (40, 400));
        assert_eq!((place.row, prompt), (2, Some(0)));
    }

    // `left`, `center` and `right` name no row, so for a banner they mean
    // the top, as do the top anchors and no anchor at all.
    #[test]
    fn a_banner_without_a_bottom_anchor_sits_at_the_top() {
        for (position, col) in [
            (None, 0),
            (Some(Position::Left), 0),
            (Some(Position::Center), 3),
            (Some(Position::Right), 7),
            (Some(Position::TopLeft), 0),
            (Some(Position::Top), 3),
            (Some(Position::TopRight), 7),
        ] {
            let (place, prompt) = banner_at(position, (20, 20));
            assert_eq!((place.col, place.row), (col, 0), "{position:?}");
            assert_eq!(prompt, Some(2), "{position:?}");
        }
    }

    // On a display turned a quarter turn, the geometry is the screen as
    // the text reads on it, so `bottom-right` puts the box at the end of
    // the grid's last row, wherever that falls on the panel. A panel of
    // 96×80 turned a quarter reads as 80×96, a grid of 10×6 cells.
    //
    // References:
    // - ADR-1100 D17, "Where the picture sits": the anchors on a rotated
    //   display.
    #[test]
    fn on_a_turned_display_bottom_right_ends_the_last_row() {
        let turned = Geometry {
            display: (80, 96),
            cell: (8, 16),
            grid: (10, 6),
        };
        let src = vec![0xff00_ff00; 20 * 20];
        let made = make(
            &placed(Fit::Natural, Some(Position::BottomRight), SCALE_FULL),
            &turned,
            (20, 20),
            &src,
        )
        .expect("made");
        assert_eq!(picture_on_grid(&made, turned.cell), (60, 76, 20, 20));
        assert_eq!(
            (
                made.place.col + made.place.cols,
                made.place.row + made.place.rows
            ),
            (10, 6)
        );
    }
}
