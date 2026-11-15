//! The login picture: what vtium makes of a decoded picture before it
//! places the picture at a login prompt, as a background beneath the text
//! or as a banner above the prompt.
//!
//! The picture arrives decoded, in the store's format: premultiplied
//! ARGB, one `u32` a pixel. [`make`] fits it to the form the settings
//! give, scales it once into exactly the pixels the screen shows, and
//! says where the placement goes. Every display path then draws the same
//! stored pixels, so the CPU and GPU paths start from the same bytes.
//!
//! The scaler is an area resampler in integer arithmetic. Each pixel it
//! writes is the average of the source pixels under that pixel's area,
//! each weighted by how much of the area it covers. At a scale of one
//! every pixel is copied exactly, so a picture made at the display's own
//! resolution draws one to one. Averaging premultiplied pixels keeps every
//! color within its alpha.
//!
//! References:
//! - ADR-1100 D17: the two forms, the fits, the banner's size, position
//!   and limits, and the placement at the lowest z.

use crate::image::{NewImage, Place};
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
    /// At its own size, centered; what passes the edges is cut off.
    Natural,
    /// A banner at the top of the screen, with the login prompt below it.
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

/// Where a banner sits across the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    Left,
    Center,
    Right,
}

impl Position {
    /// The position `login-image-position`'s word names: `left`, `center`
    /// or `right`.
    pub fn parse(word: &str) -> Option<Position> {
        match word {
            "left" => Some(Position::Left),
            "center" => Some(Position::Center),
            "right" => Some(Position::Right),
            _ => None,
        }
    }
}

/// The form the settings give the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Form {
    pub fit: Fit,
    /// A banner's height in text rows; `None` for its own size.
    pub rows: Option<u32>,
    pub position: Position,
}

/// The screen the picture is made for: the display's size in pixels, the
/// character cell in pixels, and the grid in cells. The renderer centers
/// the grid on the display, with the odd spare pixel after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub display: (u32, u32),
    pub cell: (u32, u32),
    pub grid: (u32, u32),
}

/// A picture ready to place: its pixels, which the store keeps, where the
/// placement goes, and for a banner the row the login prompt starts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Made {
    pub width: u32,
    pub height: u32,
    /// Premultiplied ARGB, `width` × `height` of them.
    pub pixels: Vec<u32>,
    pub opaque: bool,
    pub place: Place,
    /// The row, from 0, below a banner; `None` for a background.
    pub prompt_row: Option<u32>,
}

impl Made {
    /// The image the store takes: a kitty picture with no image id and no
    /// image number, so no program can name it.
    pub fn image(&self) -> NewImage {
        NewImage {
            protocol: Protocol::KITTY,
            width: self.width,
            height: self.height,
            pixels: self.pixels.clone(),
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

/// The picture `source`, `size` pixels, made for `form` on `geometry`.
/// `None` when the screen has no room for it: a grid of no cells, a cell
/// of no pixels, or a banner on a grid of 2 rows or fewer.
pub fn make(form: &Form, geometry: &Geometry, size: (u32, u32), source: &[u32]) -> Option<Made> {
    let (cw, ch) = geometry.cell;
    let (cols, rows) = geometry.grid;
    let (w, h) = size;
    if cw == 0 || ch == 0 || cols == 0 || rows == 0 || w == 0 || h == 0 {
        return None;
    }
    let len = usize::try_from(u64::from(w).checked_mul(u64::from(h))?).ok()?;
    if source.len() != len {
        return None;
    }
    let grid_px = (cols.checked_mul(cw)?, rows.checked_mul(ch)?);
    if form.fit == Fit::Banner {
        return banner(form, geometry, size, source, grid_px);
    }
    let (dw, dh) = geometry.display;
    let display = (dw.max(grid_px.0), dh.max(grid_px.1));
    let scaled = fitted(form.fit, size, display)?;
    // the scaled picture and the grid are both centered on the display;
    // the output is the part of the picture that falls on the grid
    let offset = (
        centered(display.0, scaled.0).saturating_sub(centered(display.0, grid_px.0)),
        centered(display.1, scaled.1).saturating_sub(centered(display.1, grid_px.1)),
    );
    let pixels = resample(source, size, scaled, offset, grid_px)?;
    let opaque = pixels.iter().all(|p| p >> 24 == 0xff);
    Some(Made {
        width: grid_px.0,
        height: grid_px.1,
        pixels,
        opaque,
        place: Place {
            cols,
            rows,
            z: Z,
            ..Place::default()
        },
        prompt_row: None,
    })
}

/// The banner form: its own size or `form.rows` tall, scaled down to fit
/// the grid's width and all but two of its rows, at the column its
/// position gives.
fn banner(
    form: &Form,
    geometry: &Geometry,
    (w, h): (u32, u32),
    source: &[u32],
    (max_w, grid_h): (u32, u32),
) -> Option<Made> {
    let (cw, ch) = geometry.cell;
    let (cols, rows) = geometry.grid;
    let max_h = rows.checked_sub(2)?.checked_mul(ch)?;
    if max_h == 0 || grid_h == 0 {
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
    let pixels = resample(source, (w, h), (tw, th), (0, 0), (tw, th))?;
    let opaque = pixels.iter().all(|p| p >> 24 == 0xff);
    let box_cols = tw.div_ceil(cw);
    let box_rows = th.div_ceil(ch);
    let spare = cols.saturating_sub(box_cols);
    let col = match form.position {
        Position::Left => 0,
        // an odd number of spare columns leaves the extra one on the right
        Position::Center => spare / 2,
        Position::Right => spare,
    };
    Some(Made {
        width: tw,
        height: th,
        pixels,
        opaque,
        place: Place {
            col,
            z: Z,
            ..Place::default()
        },
        prompt_row: Some(box_rows),
    })
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
    source: &[u32],
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
                let line = source.get(start..start.checked_add(width)?)?;
                let row = across_row(line, &across, out_w);
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

/// One source row, `line`, resampled across through `across`.
fn across_row(line: &[u32], across: &Taps, out_w: usize) -> Row {
    (0..out_w)
        .map(|i| {
            let mut px = [0u32; 4];
            if let Some(cols) = across.of(i) {
                for (k, wx) in cols {
                    let ch = channels(line.get(k).copied().unwrap_or(0));
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
    reason = "a test fails by panicking"
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
            position: Position::Left,
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
        assert_eq!(made.pixels, src);
        assert!(made.opaque);
        assert_eq!(
            (made.place.cols, made.place.rows, made.place.z),
            (10, 3, i32::MIN)
        );
        assert_eq!(made.prompt_row, None);
    }

    #[test]
    fn a_background_keeps_the_part_that_falls_on_the_grid() {
        // a display of 84x50 with a grid of 80x48 centered on it, 2 pixel
        // columns and 1 pixel row before it: the picture of 84x50 at one to
        // one, cut to the grid
        let src = picture(84, 50);
        let geo = Geometry {
            display: (84, 50),
            ..SCREEN
        };
        let made = make(&form(Fit::Cover), &geo, (84, 50), &src).expect("made");
        assert_eq!((made.width, made.height), (80, 48));
        for y in 0..48 {
            for x in 0..80 {
                assert_eq!(made.pixels[y * 80 + x], src[(y + 1) * 84 + x + 2]);
            }
        }
    }

    #[test]
    fn contain_leaves_transparent_bands_and_cover_crops() {
        // a square picture on a display twice as wide as tall
        let src = vec![0xffff_0000; 16 * 16];
        let made = make(&form(Fit::Contain), &SCREEN, (16, 16), &src).expect("made");
        assert!(!made.opaque);
        // 48 tall, so 48 wide, centered: 16 transparent columns each side
        assert_eq!(made.pixels[0], 0);
        assert_eq!(made.pixels[15], 0);
        assert_eq!(made.pixels[16], 0xffff_0000);
        assert_eq!(made.pixels[63], 0xffff_0000);
        assert_eq!(made.pixels[64], 0);
        let made = make(&form(Fit::Cover), &SCREEN, (16, 16), &src).expect("made");
        assert!(made.opaque);
    }

    #[test]
    fn stretch_fills_and_none_centres_at_its_own_size() {
        let src = vec![0xff00_ff00; 4 * 4];
        let made = make(&form(Fit::Stretch), &SCREEN, (4, 4), &src).expect("made");
        assert!(made.opaque);
        let made = make(&form(Fit::Natural), &SCREEN, (4, 4), &src).expect("made");
        // 4x4 centered on 80x48: columns 38 to 41, rows 22 to 25
        let at = |x: usize, y: usize| made.pixels[y * 80 + x];
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
        let out = resample(&src, (4, 4), (2, 2), (0, 0), (2, 2)).expect("resampled");
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
        assert_eq!(made.pixels, src);
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
                position,
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

    #[test]
    fn the_words_of_the_keys() {
        assert_eq!(Fit::parse("none"), Some(Fit::Natural));
        assert_eq!(Fit::parse("banner"), Some(Fit::Banner));
        assert_eq!(Fit::parse("fill"), None);
        assert_eq!(Position::parse("center"), Some(Position::Center));
        assert_eq!(Position::parse("middle"), None);
    }
}
