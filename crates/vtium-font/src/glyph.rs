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
    /// color glyph carries no box: it fits its cells (ADR-1302 D5).
    pub fn with_ink(
        mut self,
        (left, top): (i32, i32),
        (width, height): (u32, u32),
        pixels: Vec<u8>,
    ) -> Option<Glyph> {
        if self.format != Format::Mask {
            return None;
        }
        let cell = self.width.checked_div(self.cells.cells())?;
        let boxed = crate::set::format::InkRect {
            left: i64::from(left),
            top: i64::from(top),
            width: i64::from(width),
            height: i64::from(height),
        };
        let span = (i64::from(self.width), i64::from(self.height));
        if !boxed.in_reach((i64::from(cell), i64::from(self.height)), span) {
            return None;
        }
        let n = usize::try_from(width.checked_mul(height)?).ok()?;
        if pixels.len() != n {
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

    /// This glyph boxed to `width` pixels and marked as spanning `cells`:
    /// its pixels unchanged when it is that wide already, centred when it
    /// is narrower, the odd spare pixel after it, and squeezed sideways
    /// when it is wider, so no ink is cut (ADR-1302 D14). A glyph boxed to
    /// another width loses its ink box, which was placed for its own
    /// cells.
    ///
    /// `unit` is the side of one of the font's pixels in the glyph's
    /// pixels: a bitmap face's whole scale, else 1. A squeeze works in the
    /// font's own pixels whenever both widths are whole numbers of them,
    /// so a glyph drawn at a scale squeezes to that scale's drawing of the
    /// glyph squeezed at its own size.
    pub(crate) fn fit(self, width: u32, cells: Width, unit: u32) -> Option<Glyph> {
        if width == self.width {
            return Some(self.with_cells(cells));
        }
        if width < self.width {
            return self.squeezed(width, unit).map(|g| g.with_cells(cells));
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

    /// This glyph scaled sideways to `width` pixels, narrower than it, in
    /// the font's pixels `unit` across where both widths are whole
    /// numbers of them. Where the glyph's width is a whole multiple of
    /// `width`, each of a mask's columns takes the strongest pixel of the
    /// columns it covers, so a 1-bit glyph stays 1-bit and on its font's
    /// grid. Otherwise, and for a color glyph, each pixel takes the
    /// coverage of the part of the row it covers, rounded down, as the
    /// console scales a letter and its spacing marks (ADR-1302 D14).
    fn squeezed(&self, width: u32, unit: u32) -> Option<Glyph> {
        let whole_units = |n: u32| n.checked_rem(unit) == Some(0);
        let unit = if whole_units(self.width) && whole_units(width) {
            unit
        } else {
            1
        };
        let (from, to) = (self.width.checked_div(unit)?, width.checked_div(unit)?);
        let strongest = self.format == Format::Mask && from.checked_rem(to) == Some(0);
        let bpp = usize::try_from(self.format.bytes_per_pixel()).ok()?;
        let unit = usize::try_from(unit).ok()?;
        let (f, t) = (u64::from(from), u64::from(to));
        let mut out = Glyph::new(width, self.height, self.format)?;
        for (dst, src) in out.rows_mut().zip(self.rows()) {
            // the row in the font's pixels: each the strongest of the
            // `unit` pixels it is drawn as, which are alike in a glyph
            // drawn at a whole scale
            let font: Vec<&[u8]> = src.chunks_exact(bpp.checked_mul(unit)?).collect();
            let font_px = |i: u64, ch: usize| -> u64 {
                let px = usize::try_from(i).ok().and_then(|i| font.get(i));
                px.map_or(0, |px| {
                    px.iter()
                        .skip(ch)
                        .step_by(bpp)
                        .copied()
                        .max()
                        .map_or(0, u64::from)
                })
            };
            for (j, out_px) in (0u64..).zip(dst.chunks_exact_mut(bpp.checked_mul(unit)?)) {
                // output pixel j spans [j * from, (j + 1) * from) of the
                // row's from * to parts, and font pixel i spans
                // [i * to, (i + 1) * to)
                let (start, end) = (j.checked_mul(f)?, j.checked_add(1)?.checked_mul(f)?);
                let (first, last) = (start.checked_div(t)?, end.div_ceil(t));
                for ch in 0..bpp {
                    let value = if strongest {
                        (first..last).map(|i| font_px(i, ch)).max().unwrap_or(0)
                    } else {
                        let mut sum = 0u64;
                        for i in first..last {
                            let (a, b) = (i.checked_mul(t)?, i.checked_add(1)?.checked_mul(t)?);
                            let overlap = end.min(b).saturating_sub(start.max(a));
                            sum = sum.checked_add(font_px(i, ch).checked_mul(overlap)?)?;
                        }
                        sum.checked_div(f)?
                    };
                    let value = u8::try_from(value).ok()?;
                    for px in out_px.chunks_exact_mut(bpp) {
                        *px.get_mut(ch)? = value;
                    }
                }
            }
        }
        Some(out)
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

    // ADR-1302 D14: a glyph twice as wide as its cells is squeezed into
    // them, each column the strongest of the two it takes, so every inked
    // column keeps its ink: none is cut, as GNU Unifont's 16-pixel ा in
    // one 8-pixel cell keeps its stem in column 14
    #[test]
    fn a_glyph_twice_its_cells_merges_each_pair_of_columns() {
        let ink = [(0, 255), (5, 80), (6, 255), (11, 30), (14, 255)];
        let g = columns(16, |x| {
            ink.iter().find(|&&(at, _)| at == x).map_or(0, |&(_, a)| a)
        });
        let out = g.fit(8, Width::Narrow, 1).expect("fitted");
        assert_eq!((out.width(), out.cells()), (8, Width::Narrow));
        assert_eq!(first_row(&out), [255, 0, 80, 255, 0, 30, 0, 255]);
        assert_eq!(out.rows().nth(1), out.rows().next());
    }

    // ADR-1302 D14: a glyph wider than its cells by a ratio that is not a
    // whole number is scaled to them by coverage, as the console scales a
    // letter and its spacing marks, so the ink at both edges stays: 10
    // columns into 8, each edge column takes eight tenths of the inked
    // column it covers most of, 255 * 8 / 10
    #[test]
    fn a_glyph_wider_by_a_ratio_that_is_not_whole_is_scaled_by_coverage() {
        let g = columns(10, |x| if x == 0 || x == 9 { 255 } else { 0 });
        let out = g.fit(8, Width::Narrow, 1).expect("fitted");
        assert_eq!(first_row(&out), [204, 0, 0, 0, 0, 0, 0, 204]);
        let full = columns(10, |_| 255);
        let out = full.fit(8, Width::Narrow, 1).expect("fitted");
        assert_eq!(first_row(&out), [255; 8]);
    }

    // a color glyph squeezed by a whole ratio averages its pixels, which
    // are premultiplied, instead of taking each channel's strongest
    #[test]
    fn a_color_glyph_squeezed_averages_its_pixels() {
        let px = [
            [200, 0, 0, 200],
            [0, 0, 100, 100],
            [0, 0, 0, 0],
            [40, 40, 40, 255],
        ];
        let g = Glyph::from_pixels(4, 1, Format::Rgba, px.concat()).expect("a glyph");
        let out = g.fit(2, Width::Narrow, 1).expect("fitted");
        assert_eq!(out.pixels(), [100, 0, 50, 150, 20, 20, 20, 127]);
    }

    // a glyph as wide as its cells keeps its pixels and its ink box, and
    // only takes the cells it is marked with
    #[test]
    fn a_glyph_as_wide_as_its_cells_is_unchanged() {
        let g = columns(16, |x| (x * 16) as u8)
            .with_ink((-2, 0), (20, 2), vec![9; 40])
            .expect("an ink box");
        let out = g.clone().fit(16, Width::Wide, 1).expect("fitted");
        assert_eq!(out.pixels(), g.pixels());
        assert_eq!(out.ink(), g.ink());
        assert_eq!(out.cells(), Width::Wide);
    }

    // a glyph narrower than its cells is centred in them, the odd spare
    // pixel after it
    #[test]
    fn a_glyph_narrower_than_its_cells_is_centred() {
        let g = columns(5, |_| 255);
        let out = g.fit(8, Width::Narrow, 1).expect("fitted");
        assert_eq!(first_row(&out), [0, 255, 255, 255, 255, 255, 0, 0]);
    }
}
