//! [`Bitmap`]: an 8-bit coverage mask, the form every glyph and sprite
//! takes.

use crate::bounds::CELL_SIDE_MAX;

/// A glyph or sprite: one byte of coverage per pixel, 0 for none and 255
/// for full, row by row from the top left. The renderer blends it in the
/// cell's colours.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Bitmap {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Bitmap {
    /// A blank bitmap, or `None` when a side is 0, the height passes
    /// [`CELL_SIDE_MAX`], or the width passes two cells of it.
    pub fn new(width: u32, height: u32) -> Option<Bitmap> {
        if width == 0
            || height == 0
            || height > CELL_SIDE_MAX
            || width > CELL_SIDE_MAX.saturating_mul(2)
        {
            return None;
        }
        let len = usize::try_from(width.checked_mul(height)?).ok()?;
        Some(Bitmap {
            width,
            height,
            pixels: vec![0; len],
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

    /// Every pixel, row by row.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The rows, from the top.
    pub fn rows(&self) -> impl Iterator<Item = &[u8]> {
        self.pixels.chunks_exact(self.width_usize())
    }

    pub(crate) fn rows_mut(&mut self) -> impl Iterator<Item = &mut [u8]> {
        let w = self.width_usize();
        self.pixels.chunks_exact_mut(w)
    }

    /// The pixel at `x`, `y`, if it is inside.
    pub fn get(&self, x: u32, y: u32) -> Option<u8> {
        if x >= self.width {
            return None;
        }
        let i = self.index(y)?.checked_add(usize::try_from(x).ok()?)?;
        self.pixels.get(i).copied()
    }

    pub(crate) fn get_mut(&mut self, x: u32, y: u32) -> Option<&mut u8> {
        if x >= self.width {
            return None;
        }
        let i = self.index(y)?.checked_add(usize::try_from(x).ok()?)?;
        self.pixels.get_mut(i)
    }

    /// Sets `[x0, x1)` by `[y0, y1)` to `alpha`, clipped to the bitmap.
    pub(crate) fn fill(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, alpha: u8) {
        let x1 = x1.min(self.width);
        let y1 = y1.min(self.height);
        let (Ok(a), Ok(b)) = (usize::try_from(x0), usize::try_from(x1)) else {
            return;
        };
        if b <= a {
            return;
        }
        for y in y0..y1 {
            let Some(start) = self.index(y) else {
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

    /// This bitmap boxed to `width`, centred, with what overflows cut off
    /// evenly from both sides; itself when it is that wide already. The C
    /// reference's `fit_glyph` (font.c:267-308).
    pub(crate) fn fit(self, width: u32) -> Option<Bitmap> {
        if width == self.width {
            return Some(self);
        }
        let mut out = Bitmap::new(width, self.height)?;
        let n = usize::try_from(self.width.min(width)).ok()?;
        let dst = usize::try_from(width.saturating_sub(self.width) / 2).ok()?;
        let src = usize::try_from(self.width.saturating_sub(width) / 2).ok()?;
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

    fn width_usize(&self) -> usize {
        // a width of 0 is never built; chunks_exact needs at least 1
        usize::try_from(self.width).unwrap_or(1).max(1)
    }

    /// The index of row `y`'s first pixel, if `y` is inside.
    fn index(&self, y: u32) -> Option<usize> {
        if y >= self.height {
            return None;
        }
        usize::try_from(y).ok()?.checked_mul(self.width_usize())
    }
}
