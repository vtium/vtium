//! [`Glyph`]: what every face and sprite draws, a coverage mask or
//! premultiplied RGBA.

use crate::bounds::CELL_SIDE_MAX;
use crate::face::Width;

/// How a glyph's pixels are stored.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// One byte of coverage a pixel, 0 for none and 255 for full. The
    /// renderer blends it in the cell's colours. Bitmap fonts and the
    /// sprites draw only masks.
    Mask,
    /// Four bytes a pixel, red, green, blue and alpha, the colours already
    /// multiplied by alpha, so no colour byte exceeds its alpha. A colour
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
    /// colour byte greater than its alpha, which is not premultiplied.
    pub fn from_pixels(width: u32, height: u32, format: Format, pixels: Vec<u8>) -> Option<Glyph> {
        if pixels.len() != len(width, height, format)? {
            return None;
        }
        if format == Format::Rgba
            && pixels.chunks_exact(4).any(|p| match p {
                &[r, g, b, a] => r > a || g > a || b > a,
                _ => true,
            })
        {
            return None;
        }
        Some(Glyph {
            width,
            height,
            cells: Width::Narrow,
            format,
            pixels,
        })
    }

    fn new(width: u32, height: u32, format: Format) -> Option<Glyph> {
        Some(Glyph {
            width,
            height,
            cells: Width::Narrow,
            format,
            pixels: vec![0; len(width, height, format)?],
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

    /// This glyph boxed to `width` pixels and marked as spanning `cells`,
    /// centred, with what overflows cut off evenly from both sides; the
    /// pixels unchanged when it is that wide already. The C reference's
    /// `fit_glyph` (font.c:267-308).
    pub(crate) fn fit(self, width: u32, cells: Width) -> Option<Glyph> {
        if width == self.width {
            return Some(self.with_cells(cells));
        }
        let mut out = Glyph::new(width, self.height, self.format)?.with_cells(cells);
        let bpp = self.format.bytes_per_pixel();
        let bytes = |px: u32| usize::try_from(px.saturating_mul(bpp)).ok();
        let n = bytes(self.width.min(width))?;
        let dst = bytes(width.saturating_sub(self.width) / 2)?;
        let src = bytes(self.width.saturating_sub(width) / 2)?;
        for (to, from) in out.rows_mut().zip(self.rows()) {
            if let (Some(to), Some(from)) = (
                to.get_mut(dst..dst.saturating_add(n)),
                from.get(src..src.saturating_add(n)),
            ) {
                to.copy_from_slice(from);
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
