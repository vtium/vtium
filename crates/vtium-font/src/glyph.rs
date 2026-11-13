//! [`Glyph`]: what every face and sprite draws, a coverage mask or
//! premultiplied RGBA.

use crate::bounds::CELL_SIDE_MAX;
use crate::face::Width;
#[cfg(doc)]
use crate::reach::{REACH_CELLS, REACH_ROWS};

/// How a glyph's pixels are stored.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// One byte of coverage a pixel, 0 for none and 255 for full. The
    /// renderer blends it in the cell's colors. Bitmap fonts and the
    /// sprites draw only masks.
    Mask,
    /// Four bytes a pixel, red, green, blue and alpha, the colors already
    /// multiplied by alpha, so no color byte exceeds its alpha. A color
    /// face such as emoji draws these (vtium/vtium#23, #24).
    Rgba,
}

impl Format {
    /// Bytes a pixel: 1 or 4.
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            Format::Mask => 1,
            Format::Rgba => 4,
        }
    }
}

/// How [`Glyph::narrowed`] narrows a mask sideways (ADR-1302 D14).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Narrowing {
    /// OR pooling: a column of the result is inked where any column of
    /// the glyph that it covers is inked, at the strongest of them. A
    /// 1-bit glyph stays 1-bit, sharp and with no gray. Bitmap glyphs take
    /// it.
    Pool,
    /// Each pixel takes the coverage of the part of the row it covers,
    /// rounded down. Outline glyphs take it.
    Coverage,
}

/// A drawn glyph: its size in pixels, the cells it spans and its pixels,
/// row by row from the top left. Both formats are bounded alike: a side of
/// at least 1, a height of at most [`CELL_SIDE_MAX`], and a width of at
/// most two cells of it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Glyph {
    width: u32,
    height: u32,
    cells: Width,
    format: Format,
    pixels: Vec<u8>,
    ink: Option<Box<Ink>>,
}

/// The ink box of a glyph whose ink passes its cells (ADR-1302 D1, D6;
/// ADR-1306 D3): [`Ink::width`] columns from [`Ink::left`], the offset of
/// its first column from the left edge of the glyph's first cell, 0 or
/// negative, and [`Ink::height`] rows from [`Ink::top`], the offset of its
/// first row from the top of the glyph's row, 0 or negative, in the
/// glyph's format. It spans the glyph's cells and the ink past them, and
/// reaches at most [`REACH_CELLS`] cells past them on each side and
/// [`REACH_ROWS`] rows above and below them. The glyph's own pixels stay
/// its ink cut to its cells, so a renderer that draws no box draws them
/// as before.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ink {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Ink {
    /// The offset of the box's first column from the left edge of the
    /// glyph's first cell: 0 or negative.
    pub fn left(&self) -> i32 {
        self.left
    }

    /// The offset of the box's first row from the top of the glyph's
    /// row: 0 or negative.
    pub fn top(&self) -> i32 {
        self.top
    }

    /// Its width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Its height in pixels: the glyph's height when its ink stays inside
    /// its row.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Every pixel's bytes, row by row, in the glyph's format.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

/// The byte length of a `width` by `height` glyph in `format`, or `None`
/// past the bounds.
fn len(width: u32, height: u32, format: Format) -> Option<usize> {
    if width == 0
        || height == 0
        || height > CELL_SIDE_MAX
        || width > CELL_SIDE_MAX.saturating_mul(2)
    {
        return None;
    }
    let n = width
        .checked_mul(height)?
        .checked_mul(format.bytes_per_pixel())?;
    usize::try_from(n).ok()
}

impl Glyph {
    /// A blank mask one cell wide, or `None` past the bounds.
    pub fn mask(width: u32, height: u32) -> Option<Glyph> {
        Glyph::new(width, height, Format::Mask)
    }

    /// A transparent RGBA glyph one cell wide, or `None` past the bounds.
    pub fn rgba(width: u32, height: u32) -> Option<Glyph> {
        Glyph::new(width, height, Format::Rgba)
    }

    /// A glyph holding `pixels` in `format`, or `None` past the bounds,
    /// when `pixels` is not exactly the glyph's length, or for RGBA with a
    /// color byte greater than its alpha, which is not premultiplied.
    pub fn from_pixels(width: u32, height: u32, format: Format, pixels: Vec<u8>) -> Option<Glyph> {
        if pixels.len() != len(width, height, format)? {
            return None;
        }
        if format == Format::Rgba
            && pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|&[r, g, b, a]| r > a || g > a || b > a)
        {
            return None;
        }
        Some(Glyph {
            width,
            height,
            cells: Width::Narrow,
            format,
            pixels,
            ink: None,
        })
    }

    pub(crate) fn new(width: u32, height: u32, format: Format) -> Option<Glyph> {
        Some(Glyph {
            width,
            height,
            cells: Width::Narrow,
            format,
            pixels: vec![0; len(width, height, format)?],
            ink: None,
        })
    }

    /// Its width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Its height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// How many cells it spans.
    pub fn cells(&self) -> Width {
        self.cells
    }

    /// The same glyph marked as spanning `cells`.
    pub fn with_cells(mut self, cells: Width) -> Glyph {
        self.cells = cells;
        self
    }

    /// Its ink box, when its ink passes its cells (ADR-1302 D6).
    pub fn ink(&self) -> Option<&Ink> {
        self.ink.as_deref()
    }

    /// The same glyph with the ink box `width` columns from `left` and
    /// `height` rows from `top` ([`Ink`]), holding `pixels` in the glyph's
    /// format; `None` when the box is not past the glyph's cells on any
    /// side, passes its reach, or `pixels` is not the box's length. A
    /// color glyph carries a box only where it is a color emoji drawn in
    /// one cell, which reaches 1 pixel into each neighbour (ADR-1302 D5).
    pub fn with_ink(
        mut self,
        (left, top): (i32, i32),
        (width, height): (u32, u32),
        pixels: Vec<u8>,
    ) -> Option<Glyph> {
        let cell = self.width.checked_div(self.cells.cells())?;
        let boxed = crate::set::format::InkRect {
            left: i64::from(left),
            top: i64::from(top),
            width: i64::from(width),
            height: i64::from(height),
        };
        let wide = self.cells != Width::Narrow;
        let at = (i64::from(cell), i64::from(self.height));
        if self.format == Format::Rgba && !boxed.is_emoji_edge(at, wide) {
            return None;
        }
        let span = (i64::from(self.width), i64::from(self.height));
        if !boxed.in_reach((i64::from(cell), i64::from(self.height)), span) {
            return None;
        }
        let n = width
            .checked_mul(height)?
            .checked_mul(self.format.bytes_per_pixel())?;
        if pixels.len() != usize::try_from(n).ok()? {
            return None;
        }
        self.ink = Some(Box::new(Ink {
            left,
            top,
            width,
            height,
            pixels,
        }));
        Some(self)
    }

    /// How its pixels are stored.
    pub fn format(&self) -> Format {
        self.format
    }

    /// Every pixel's bytes, row by row.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The rows, from the top, each `width` pixels of the format's bytes.
    pub fn rows(&self) -> impl Iterator<Item = &[u8]> {
        self.pixels.chunks_exact(self.stride())
    }

    pub(crate) fn rows_mut(&mut self) -> impl Iterator<Item = &mut [u8]> {
        let stride = self.stride();
        self.pixels.chunks_exact_mut(stride)
    }

    /// The bytes of the pixel at `x`, `y`, if it is inside: one for a
    /// mask, four for RGBA.
    pub fn pixel(&self, x: u32, y: u32) -> Option<&[u8]> {
        let i = self.offset(x, y)?;
        let bpp = usize::try_from(self.format.bytes_per_pixel()).ok()?;
        self.pixels.get(i..i.checked_add(bpp)?)
    }

    /// The bytes of the pixel at `x`, `y`, to write, if it is inside.
    pub(crate) fn pixel_mut(&mut self, x: u32, y: u32) -> Option<&mut [u8]> {
        let i = self.offset(x, y)?;
        let bpp = usize::try_from(self.format.bytes_per_pixel()).ok()?;
        self.pixels.get_mut(i..i.checked_add(bpp)?)
    }

    /// A mask's coverage at `x`, `y`; `None` outside it or for RGBA.
    pub(crate) fn coverage_mut(&mut self, x: u32, y: u32) -> Option<&mut u8> {
        if self.format != Format::Mask {
            return None;
        }
        let i = self.offset(x, y)?;
        self.pixels.get_mut(i)
    }

    /// Sets a mask's `[x0, x1)` by `[y0, y1)` to `alpha`, clipped to the
    /// glyph; does nothing to RGBA.
    pub(crate) fn fill(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, alpha: u8) {
        if self.format != Format::Mask {
            return;
        }
        let x1 = x1.min(self.width);
        let y1 = y1.min(self.height);
        let (Ok(a), Ok(b)) = (usize::try_from(x0), usize::try_from(x1)) else {
            return;
        };
        if b <= a {
            return;
        }
        for y in y0..y1 {
            let Some(start) = self.offset(0, y) else {
                return;
            };
            if let Some(span) = self
                .pixels
                .get_mut(start.saturating_add(a)..start.saturating_add(b))
            {
                span.fill(alpha);
            }
        }
    }

    /// This glyph fitted to `width` pixels, the width of `cells` cells,
    /// and marked as spanning them (ADR-1302 D14, D5, ADR-1200 D7). A glyph
    /// that wide keeps its pixels, and a narrower one is centred, the odd
    /// spare pixel after it. No column of a wider mask is ever cut:
    ///
    /// - a mask exactly twice `width` is squeezed 2:1 into its cells by OR
    ///   pooling ([`Narrowing::Pool`]), with no ink past them;
    /// - a mask at most `width` and 2 pixels more keeps its size, centred
    ///   on its cells, the odd pixel past their right edge;
    /// - any other wider mask is squeezed by OR pooling to `width` and 2
    ///   pixels more, centred, so it reaches 1 pixel past each side.
    ///
    /// A mask that passes its cells carries the columns past them in its
    /// ink box (ADR-1302 D6), kept only where they hold ink. Its rows are
    /// left as they are. A color glyph wider than `width` is a picture made
    /// for more cells, drawn in them as [`Glyph::in_one_cell`] draws it. A
    /// glyph fitted to another width loses its ink box, which was placed
    /// for its own cells. Every width counts the glyph's pixels as they are
    /// drawn, the screen's pixels, whatever scale drew the glyph.
    pub fn fit(self, width: u32, cells: Width) -> Option<Glyph> {
        if width == self.width {
            return Some(self.with_cells(cells));
        }
        if width < self.width {
            return match self.format {
                Format::Mask => self.squeezed(width, cells),
                Format::Rgba => self.in_one_cell((width, self.height)),
            }
            .map(|g| g.with_cells(cells));
        }
        let mut out = Glyph::new(width, self.height, self.format)?.with_cells(cells);
        let bpp = self.format.bytes_per_pixel();
        let bytes = |px: u32| usize::try_from(px.saturating_mul(bpp)).ok();
        let n = bytes(self.width)?;
        let dst = bytes(width.saturating_sub(self.width) / 2)?;
        for (to, from) in out.rows_mut().zip(self.rows()) {
            if let Some(to) = to.get_mut(dst..dst.saturating_add(n)) {
                to.copy_from_slice(from);
            }
        }
        Some(out)
    }

    /// This mask, wider than `width`, fitted to it by [`Glyph::fit`]'s
    /// rule for a mask.
    fn squeezed(&self, width: u32, cells: Width) -> Option<Glyph> {
        if self.width == width.checked_mul(2)? {
            return self.narrowed(width, Narrowing::Pool);
        }
        let room = width.checked_add(2)?;
        let pressed;
        let glyph = if self.width > room {
            pressed = self.narrowed(room, Narrowing::Pool)?;
            &pressed
        } else {
            self
        };
        glyph.overhanging(width, cells)
    }

    /// This glyph, wider than `width` by at most 2 pixels, centred on
    /// cells `width` pixels wide, the odd pixel past their right edge: its
    /// own pixels the columns inside the cells, and its ink box its cells
    /// and the columns past them that hold ink.
    fn overhanging(&self, width: u32, cells: Width) -> Option<Glyph> {
        let bpp = usize::try_from(self.format.bytes_per_pixel()).ok()?;
        let over = self.width.checked_sub(width)?;
        let left = over / 2;
        let at = |px: u32| usize::try_from(px).ok()?.checked_mul(bpp);
        let (from, n) = (at(left)?, at(width)?);
        let mut own = Vec::with_capacity(n.checked_mul(usize::try_from(self.height).ok()?)?);
        for row in self.rows() {
            own.extend_from_slice(row.get(from..from.checked_add(n)?)?);
        }
        let glyph = Glyph::from_pixels(width, self.height, self.format, own)?.with_cells(cells);
        // the box: the cells, and the columns past them that hold ink
        let inked = |x: u32| {
            self.rows().any(|row| {
                at(x)
                    .and_then(|i| row.get(i..i.checked_add(bpp)?))
                    .is_some_and(|px| px.iter().any(|&b| b > 0))
            })
        };
        let first = if left > 0 && inked(0) { 0 } else { left };
        let end = left.checked_add(width)?;
        let last = if self.width > end && inked(self.width.saturating_sub(1)) {
            self.width
        } else {
            end
        };
        if (first, last) == (left, end) {
            return Some(glyph);
        }
        let (bx, bn) = (at(first)?, at(last.checked_sub(first)?)?);
        let mut boxed = Vec::with_capacity(bn.checked_mul(usize::try_from(self.height).ok()?)?);
        for row in self.rows() {
            boxed.extend_from_slice(row.get(bx..bx.checked_add(bn)?)?);
        }
        let offset = i32::try_from(first)
            .ok()?
            .checked_sub(i32::try_from(left).ok()?)?;
        glyph.with_ink((offset, 0), (last.checked_sub(first)?, self.height), boxed)
    }

    /// This mask narrowed sideways to `width` pixels, no wider than it,
    /// its rows unchanged: by OR pooling, which keeps a 1-bit glyph 1-bit,
    /// or by coverage, which an outline glyph takes ([`Narrowing`]). Output
    /// column j spans source columns `j * from / width` to `(j + 1) * from
    /// / width`. The glyph path ([`Glyph::fit`]) and the console's drawing
    /// of a letter with its spacing marks both narrow a glyph here, so the
    /// same squeeze never draws two different ways (ADR-1302 D14). `None`
    /// for a color glyph, or a `width` of 0 or wider than the mask.
    pub fn narrowed(&self, width: u32, how: Narrowing) -> Option<Glyph> {
        if self.format != Format::Mask || width == 0 || width > self.width {
            return None;
        }
        let (f, t) = (u64::from(self.width), u64::from(width));
        let mut out = Glyph::new(width, self.height, Format::Mask)?;
        for (dst, src) in out.rows_mut().zip(self.rows()) {
            let px = |i: u64| {
                usize::try_from(i)
                    .ok()
                    .and_then(|i| src.get(i))
                    .map_or(0, |&p| u64::from(p))
            };
            for (j, out_px) in (0u64..).zip(dst.iter_mut()) {
                // output pixel j spans [j * from, (j + 1) * from) of the
                // row's from * to parts, and source pixel i spans
                // [i * to, (i + 1) * to)
                let (start, end) = (j.checked_mul(f)?, j.checked_add(1)?.checked_mul(f)?);
                let (first, last) = (start.checked_div(t)?, end.div_ceil(t));
                let value = match how {
                    Narrowing::Pool => (first..last).map(px).max().unwrap_or(0),
                    Narrowing::Coverage => {
                        let mut sum = 0u64;
                        for i in first..last {
                            let (a, b) = (i.checked_mul(t)?, i.checked_add(1)?.checked_mul(t)?);
                            let overlap = end.min(b).saturating_sub(start.max(a));
                            sum = sum.checked_add(px(i).checked_mul(overlap)?)?;
                        }
                        sum.checked_div(f)?
                    }
                };
                *out_px = u8::try_from(value).ok()?;
            }
        }
        Some(out)
    }

    /// This color picture drawn in one cell `width` by `height` pixels
    /// (ADR-1302 D5): the square around its ink, shrunk by nearest
    /// neighbour to a square `width` and 2 pixels across, or the cell's
    /// `height` where that is less, and centred on the cell. Each pixel of
    /// the square takes the picture's pixel under its centre, so no colors
    /// are averaged. The columns past the cell go to the glyph's ink box,
    /// which reaches 1 pixel into the cell on each side. A picture with no
    /// ink gives a blank cell. `None` for a mask.
    pub fn in_one_cell(&self, (width, height): (u32, u32)) -> Option<Glyph> {
        if self.format != Format::Rgba {
            return None;
        }
        let blank = Glyph::new(width, height, Format::Rgba)?;
        let alpha = |x: u32, y: u32| self.pixel(x, y).and_then(|p| p.get(3).copied());
        let inked = |x: u32, y: u32| alpha(x, y).is_some_and(|a| a > 0);
        let rows: Vec<u32> = (0..self.height)
            .filter(|&y| (0..self.width).any(|x| inked(x, y)))
            .collect();
        let cols: Vec<u32> = (0..self.width)
            .filter(|&x| (0..self.height).any(|y| inked(x, y)))
            .collect();
        let (Some(&top), Some(&bottom), Some(&left), Some(&right)) =
            (rows.first(), rows.last(), cols.first(), cols.last())
        else {
            return Some(blank);
        };
        let (iw, ih) = (
            i64::from(right.checked_sub(left)?.checked_add(1)?),
            i64::from(bottom.checked_sub(top)?.checked_add(1)?),
        );
        // the square around the ink, centred on it
        let side = iw.max(ih);
        let sx = i64::from(left).checked_sub(side.checked_sub(iw)? / 2)?;
        let sy = i64::from(top).checked_sub(side.checked_sub(ih)? / 2)?;
        let s = width.checked_add(2)?.min(height);
        let (s64, w64, h64) = (i64::from(s), i64::from(width), i64::from(height));
        // the square on a canvas a pixel wider than the cell on each side,
        // the cell from its second column
        let x0 = w64.checked_sub(s64)?.div_euclid(2).checked_add(1)?;
        let y0 = h64.checked_sub(s64)? / 2;
        let mut canvas = Glyph::new(width.checked_add(2)?, height, Format::Rgba)?;
        // the picture's pixel under the centre of each pixel of the square
        let under = |k: i64, from: i64| -> Option<u32> {
            let at = from.checked_add(
                k.checked_mul(2)?
                    .checked_add(1)?
                    .checked_mul(side)?
                    .checked_div(s64.checked_mul(2)?)?,
            )?;
            u32::try_from(at).ok()
        };
        for j in 0..s64 {
            for i in 0..s64 {
                let (Some(x), Some(y)) = (under(i, sx), under(j, sy)) else {
                    continue;
                };
                let Some(px) = self.pixel(x, y).map(<[u8]>::to_vec) else {
                    continue;
                };
                let (cx, cy) = (x0.checked_add(i)?, y0.checked_add(j)?);
                let (Ok(cx), Ok(cy)) = (u32::try_from(cx), u32::try_from(cy)) else {
                    continue;
                };
                if let Some(to) = canvas.pixel_mut(cx, cy) {
                    to.copy_from_slice(&px);
                }
            }
        }
        canvas.overhanging(width, Width::Narrow)
    }

    /// Bytes a row; a glyph of width 0 is never built, and chunks need 1.
    fn stride(&self) -> usize {
        usize::try_from(self.width.saturating_mul(self.format.bytes_per_pixel()))
            .unwrap_or(1)
            .max(1)
    }

    /// The byte offset of the pixel at `x`, `y`, if it is inside.
    fn offset(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let bpp = self.format.bytes_per_pixel();
        let row = usize::try_from(y).ok()?.checked_mul(self.stride())?;
        row.checked_add(usize::try_from(x.checked_mul(bpp)?).ok()?)
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    /// A mask `width` pixels wide and 2 rows high, each pixel `at(x)`.
    fn columns(width: u32, at: impl Fn(u32) -> u8) -> Glyph {
        let row: Vec<u8> = (0..width).map(at).collect();
        Glyph::from_pixels(width, 2, Format::Mask, row.repeat(2)).expect("a glyph")
    }

    /// The first row of `g`.
    fn first_row(g: &Glyph) -> Vec<u8> {
        g.rows().next().expect("a row").to_vec()
    }

    /// Whether every byte of `g`'s pixels and of its ink box is 0 or 255.
    fn one_bit(g: &Glyph) -> bool {
        let box_px = g.ink().map_or(&[][..], Ink::pixels);
        g.pixels().iter().chain(box_px).all(|&p| p == 0 || p == 255)
    }

    /// The columns of the first row of `g`'s ink box, else of its pixels,
    /// as `#` and `.`, and where the box starts.
    fn drawn(g: &Glyph) -> (i32, String) {
        let (left, w, px) = match g.ink() {
            Some(ink) => (ink.left(), ink.width(), ink.pixels()),
            None => (0, g.width(), g.pixels()),
        };
        let row = px.get(..w as usize).expect("a row");
        (
            left,
            row.iter().map(|&p| if p > 0 { '#' } else { '.' }).collect(),
        )
    }

    // covers: text.overhang
    // ADR-1302 D14: a glyph exactly twice as wide as its cells is squeezed
    // into them by OR pooling, each column inked where either of the two
    // it takes is, so every inked column keeps its ink: none is cut, as
    // GNU Unifont's 16-pixel ा in one 8-pixel cell keeps its stem in
    // column 14. It carries no ink box
    #[test]
    fn a_glyph_twice_its_cells_merges_each_pair_of_columns() {
        let ink = [0, 5, 6, 11, 14];
        let g = columns(16, |x| if ink.contains(&x) { 255 } else { 0 });
        let out = g.fit(8, Width::Narrow).expect("fitted");
        assert_eq!((out.width(), out.cells()), (8, Width::Narrow));
        assert_eq!(first_row(&out), [255, 0, 255, 255, 0, 255, 0, 255]);
        assert_eq!(out.rows().nth(1), out.rows().next());
        assert!(out.ink().is_none());
    }

    // covers: text.overhang
    // ADR-1302 D14: a glyph wider than its cells by at most 2 pixels is
    // not squeezed: it is centred on its cells and reaches past them, its
    // columns past them in its ink box. Unscii-8's 8 columns in tiny5's
    // 6-pixel cell keep all 8, the first in the cell before and the 8th in
    // the cell after
    #[test]
    fn a_glyph_at_most_two_pixels_wider_keeps_every_column() {
        let g = columns(8, |x| if x == 0 || x == 7 { 255 } else { 0 });
        let out = g.fit(6, Width::Narrow).expect("fitted");
        assert_eq!(out.width(), 6);
        assert_eq!(first_row(&out), [0; 6]);
        assert_eq!(drawn(&out), (-1, "#......#".into()));
        assert!(one_bit(&out));
    }

    // ADR-1302 D14: where the glyph passes its cells by an odd number of
    // pixels, the odd pixel goes past the right edge, and a column past
    // the cells with no ink is left out of the box
    #[test]
    fn the_odd_pixel_goes_past_the_right_edge() {
        let g = columns(9, |x| if x == 8 { 255 } else { 0 });
        let out = g.fit(8, Width::Narrow).expect("fitted");
        assert_eq!(drawn(&out), (0, ".........#".get(1..).expect("9").into()));
        let blank_end = columns(8, |x| if x < 7 { 255 } else { 0 });
        let out = blank_end.fit(6, Width::Narrow).expect("fitted");
        assert_eq!(drawn(&out), (-1, "#######".into()));
    }

    // covers: text.overhang
    // ADR-1302 D14: any other glyph wider than its cells and 2 pixels more
    // is squeezed by OR pooling to that width, centred, reaching 1 pixel
    // into each neighbour: one bit, no gray, and no inked column lost. 13
    // columns into 10, around an 8-pixel cell
    #[test]
    fn a_wider_glyph_is_pooled_to_its_cells_and_two_pixels() {
        let g = columns(13, |x| if x % 3 == 0 { 255 } else { 0 });
        let out = g.fit(8, Width::Narrow).expect("fitted");
        assert!(one_bit(&out), "{:?}", out.ink());
        let (left, row) = drawn(&out);
        assert_eq!((left, row.len()), (-1, 10));
        // output column j covers source columns j * 1.3 to (j + 1) * 1.3,
        // and takes ink from any of those columns, inked at 0, 3, 6, 9, 12
        assert_eq!(row, "#.######.#");
    }

    // ADR-1302 D14: coverage narrowing, which an outline letter and its
    // spacing marks take in the console, keeps the ink at both edges: 10
    // columns into 8, each edge column takes eight tenths of the inked
    // column it covers most of, 255 * 8 / 10
    #[test]
    fn coverage_narrowing_spreads_a_column_by_its_share() {
        let g = columns(10, |x| if x == 0 || x == 9 { 255 } else { 0 });
        let out = g.narrowed(8, Narrowing::Coverage).expect("narrowed");
        assert_eq!(first_row(&out), [204, 0, 0, 0, 0, 0, 0, 204]);
        let pooled = g.narrowed(8, Narrowing::Pool).expect("narrowed");
        assert_eq!(first_row(&pooled), [255, 0, 0, 0, 0, 0, 0, 255]);
    }

    /// A 16x16 RGBA picture whose ink is the box `x0..x1` by `y0..y1`, each
    /// pixel's red its column and green its row, opaque.
    fn picture(x0: u32, x1: u32, y0: u32, y1: u32) -> Glyph {
        let mut px = Vec::new();
        for y in 0..16u32 {
            for x in 0..16u32 {
                let on = (x0..x1).contains(&x) && (y0..y1).contains(&y);
                px.extend(if on {
                    [x as u8 * 10, y as u8 * 10, 0, 255]
                } else {
                    [0; 4]
                });
            }
        }
        Glyph::from_pixels(16, 16, Format::Rgba, px).expect("a picture")
    }

    // covers: text.overhang
    // ADR-1302 D5: a color picture drawn in one 8x16 cell is the square
    // around its ink shrunk by nearest neighbour to 10x10, centred on the
    // cell, reaching 1 pixel into each neighbour through its ink box; each
    // pixel is one of the picture's, never an average
    #[test]
    fn a_color_picture_in_one_cell_is_a_square_two_pixels_wider() {
        let g = picture(2, 14, 2, 14).with_cells(Width::Wide);
        let out = g.fit(8, Width::Narrow).expect("fitted");
        assert_eq!(
            (out.width(), out.height(), out.cells()),
            (8, 16, Width::Narrow)
        );
        let ink = out.ink().expect("an ink box");
        assert_eq!(
            (ink.left(), ink.top(), ink.width(), ink.height()),
            (-1, 0, 10, 16)
        );
        let alpha = |x: usize, y: usize| ink.pixels()[(y * 10 + x) * 4 + 3];
        let inked: Vec<usize> = (0..16)
            .filter(|&y| (0..10).any(|x| alpha(x, y) > 0))
            .collect();
        assert_eq!(inked, (3..13).collect::<Vec<_>>(), "10 rows, centred in 16");
        assert!((3..13).all(|y| (0..10).all(|x| alpha(x, y) == 255)));
        // the square is the ink's 12x12 from column 2: pixel i takes
        // column 2 + (2i + 1) * 12 / 20
        let red = |x: usize| ink.pixels()[(5 * 10 + x) * 4];
        let want: Vec<u8> = (0..10)
            .map(|i| ((2 + (2 * i + 1) * 12 / 20) * 10) as u8)
            .collect();
        assert_eq!((0..10).map(red).collect::<Vec<_>>(), want);
        // the cell's own pixels are the box's middle 8 columns
        assert_eq!(
            out.pixel(0, 5),
            Some(&ink.pixels()[(5 * 10 + 1) * 4..(5 * 10 + 2) * 4])
        );
    }

    // ADR-1302 D5, D13: the square is never taller than the cell's text
    // band: in an 8x6 cell it is 6x6, inside the cell
    #[test]
    fn a_color_picture_in_a_short_cell_is_as_tall_as_the_cell() {
        let g = picture(0, 16, 0, 16).with_cells(Width::Wide);
        let short = Glyph::from_pixels(16, 6, Format::Rgba, g.pixels()[..16 * 6 * 4].to_vec())
            .expect("a picture");
        let out = short.fit(8, Width::Narrow).expect("fitted");
        assert!(out.ink().is_none());
        let alpha = |x: u32| out.pixel(x, 3).expect("a pixel")[3];
        assert_eq!(
            (0..8).map(alpha).collect::<Vec<_>>(),
            [0, 255, 255, 255, 255, 255, 255, 0]
        );
    }

    // a glyph as wide as its cells keeps its pixels and its ink box, and
    // only takes the cells it is marked with
    #[test]
    fn a_glyph_as_wide_as_its_cells_is_unchanged() {
        let g = columns(16, |x| (x * 16) as u8)
            .with_ink((-2, 0), (20, 2), vec![9; 40])
            .expect("an ink box");
        let out = g.clone().fit(16, Width::Wide).expect("fitted");
        assert_eq!(out.pixels(), g.pixels());
        assert_eq!(out.ink(), g.ink());
        assert_eq!(out.cells(), Width::Wide);
    }

    // a glyph narrower than its cells is centred in them, the odd spare
    // pixel after it
    #[test]
    fn a_glyph_narrower_than_its_cells_is_centred() {
        let g = columns(5, |_| 255);
        let out = g.fit(8, Width::Narrow).expect("fitted");
        assert_eq!(first_row(&out), [0, 255, 255, 255, 255, 255, 0, 0]);
    }
}
