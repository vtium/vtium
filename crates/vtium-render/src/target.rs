//! Where a frame is drawn: [`Target`], the display's buffer, which the
//! renderer only ever writes; [`Canvas`], a buffer in memory; and
//! [`Mapped`], a mapped dumb buffer.

/// A display buffer, written a run of pixels at a time and never read.
///
/// Each pixel is `0x00RRGGBB`, the layout of DRM's XRGB8888. The renderer
/// writes only inside the display size it was made for, and every pixel it
/// writes comes from the frame and the cache, never from what the buffer
/// held: device memory is uncached, and reading it is the slow path
/// (ADR-1300 D5). The trait has no way to read, so nothing the renderer
/// does can depend on the buffer's contents.
pub trait Target {
    /// Writes `pixels` to row `y`, starting at column `x`.
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]);
}

impl<T: Target + ?Sized> Target for &mut T {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        (**self).write(x, y, pixels);
    }
}

/// A rectangle of display pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// Its area in pixels.
    pub fn area(&self) -> u64 {
        u64::from(self.width).saturating_mul(u64::from(self.height))
    }
}

/// How the display is turned: where the top of the text faces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    /// Text reads left to right across the display.
    #[default]
    Normal,
    /// Turned a quarter clockwise: text reads top to bottom.
    Right,
    /// Turned half round.
    UpsideDown,
    /// Turned a quarter anticlockwise: text reads bottom to top.
    Left,
}

impl Orientation {
    /// Whether the text's rows run down the display's columns.
    pub(crate) fn turned(self) -> bool {
        matches!(self, Orientation::Right | Orientation::Left)
    }
}

/// A display buffer in memory, for tests and for tier 2's screens. Unlike
/// device memory it can be read back, by its owner, through
/// [`Canvas::pixels`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl Canvas {
    /// A `width` by `height` canvas of `fill`.
    pub fn new(width: u32, height: u32, fill: u32) -> Canvas {
        let n = usize::try_from(u64::from(width).saturating_mul(u64::from(height)))
            .unwrap_or(usize::MAX);
        Canvas {
            width,
            height,
            pixels: vec![fill; n],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Every pixel, row by row.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// The pixel at `x`, `y`, if it is inside.
    pub fn pixel(&self, x: u32, y: u32) -> Option<u32> {
        if x >= self.width {
            return None;
        }
        let i = usize::try_from(u64::from(y).checked_mul(u64::from(self.width))?)
            .ok()?
            .checked_add(usize::try_from(x).ok()?)?;
        self.pixels.get(i).copied()
    }
}

/// The run of `len` pixels at `x` in a row `width` long, clipped to it.
fn clip(x: u32, width: u32, len: usize) -> Option<(usize, usize)> {
    let start = usize::try_from(x).ok()?;
    let end = start.saturating_add(len).min(usize::try_from(width).ok()?);
    (start < end).then(|| (start, end.saturating_sub(start)))
}

impl Target for Canvas {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        if y >= self.height {
            return;
        }
        let Some((start, n)) = clip(x, self.width, pixels.len()) else {
            return;
        };
        let row = usize::try_from(y)
            .unwrap_or(usize::MAX)
            .saturating_mul(usize::try_from(self.width).unwrap_or(0));
        let at = row.saturating_add(start);
        if let (Some(dst), Some(src)) = (
            self.pixels.get_mut(at..at.saturating_add(n)),
            pixels.get(..n),
        ) {
            dst.copy_from_slice(src);
        }
    }
}

/// A mapped dumb buffer in DRM's XRGB8888: `stride` bytes a row, each
/// pixel four bytes, little-endian. Writes are stored and nothing is read.
#[derive(Debug)]
pub struct Mapped<'a> {
    bytes: &'a mut [u8],
    width: u32,
    height: u32,
    stride: usize,
}

impl<'a> Mapped<'a> {
    /// The buffer `bytes`, `width` by `height` pixels, `stride` bytes a row;
    /// `None` when the rows do not fit in `bytes`.
    pub fn new(bytes: &'a mut [u8], width: u32, height: u32, stride: usize) -> Option<Mapped<'a>> {
        let row = usize::try_from(width).ok()?.checked_mul(4)?;
        let rows = usize::try_from(height).ok()?;
        if row > stride || stride.checked_mul(rows)? > bytes.len() {
            return None;
        }
        Some(Mapped {
            bytes,
            width,
            height,
            stride,
        })
    }
}

impl Target for Mapped<'_> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        if y >= self.height {
            return;
        }
        let Some((start, n)) = clip(x, self.width, pixels.len()) else {
            return;
        };
        let at = usize::try_from(y)
            .unwrap_or(usize::MAX)
            .saturating_mul(self.stride)
            .saturating_add(start.saturating_mul(4));
        let Some(dst) = self
            .bytes
            .get_mut(at..at.saturating_add(n.saturating_mul(4)))
        else {
            return;
        };
        for (d, p) in dst.chunks_exact_mut(4).zip(pixels) {
            // a store of four bytes, not a copy of a slice
            if let Ok(d) = <&mut [u8; 4]>::try_from(d) {
                *d = p.to_le_bytes();
            }
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    #[test]
    fn writes_are_clipped_to_the_buffer() {
        let mut c = Canvas::new(4, 2, 0);
        c.write(2, 1, &[1, 2, 3]);
        c.write(4, 0, &[9]);
        c.write(0, 2, &[9]);
        assert_eq!(c.pixels(), &[0, 0, 0, 0, 0, 0, 1, 2]);
    }

    #[test]
    fn a_mapped_buffer_takes_little_endian_rows_at_its_stride() {
        let mut bytes = vec![0u8; 2 * 12];
        let mut m = Mapped::new(&mut bytes, 2, 2, 12).expect("fits");
        m.write(1, 1, &[0x00aa_bbcc, 0x0011_2233]);
        assert_eq!(&bytes[12..], &[0, 0, 0, 0, 0xcc, 0xbb, 0xaa, 0, 0, 0, 0, 0]);
        assert!(Mapped::new(&mut [0u8; 15], 2, 2, 8).is_none());
        assert!(Mapped::new(&mut [0u8; 64], 3, 2, 8).is_none());
    }
}
