//! [`Stored`]: a glyph or decoration as the renderer keeps it, turned to
//! the display's orientation once, at 1 bit a pixel when that loses
//! nothing, and at its font's own size with the whole factor it is drawn
//! at (ADR-1300 D2).

use vtium_font::{Format as FontFormat, Glyph};

use crate::target::Orientation;

/// How a stored glyph's pixels are held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    /// 1 bit a pixel, 0 or 255 coverage, rows padded to a byte.
    Bits,
    /// 1 byte of coverage a pixel.
    Mask,
    /// Premultiplied red, green, blue and alpha, 4 bytes a pixel.
    Rgba,
}

/// A glyph or sprite ready to draw. Its size is as stored, already turned;
/// it is drawn `scale` times larger, each pixel a square.
#[derive(Clone, Debug)]
pub(crate) struct Stored {
    width: u32,
    height: u32,
    scale: u32,
    format: Format,
    stride: usize,
    data: Vec<u8>,
}

/// `width` by `height` pixels of `bpp` bytes turned by `o`, as the C
/// reference's `picture_rotate` turned them: the pixel at column `u`, row
/// `v` of the source lands where the display's mapping of the cell puts it.
fn turn(src: &[u8], width: u32, height: u32, bpp: usize, o: Orientation) -> (u32, u32, Vec<u8>) {
    let (w, h) = (width as usize, height as usize);
    let (tw, th) = if o.turned() { (h, w) } else { (w, h) };
    let mut out = vec![0u8; src.len()];
    for (v, row) in src.chunks_exact(w.saturating_mul(bpp).max(1)).enumerate() {
        for (u, px) in row.chunks_exact(bpp).enumerate() {
            let (x, y) = match o {
                Orientation::Normal => (u, v),
                Orientation::UpsideDown => (
                    w.wrapping_sub(1).wrapping_sub(u),
                    h.wrapping_sub(1).wrapping_sub(v),
                ),
                Orientation::Right => (h.wrapping_sub(1).wrapping_sub(v), u),
                Orientation::Left => (v, w.wrapping_sub(1).wrapping_sub(u)),
            };
            let at = y.wrapping_mul(tw).wrapping_add(x).wrapping_mul(bpp);
            if let Some(dst) = out.get_mut(at..at.wrapping_add(bpp)) {
                dst.copy_from_slice(px);
            }
        }
    }
    let size = |n: usize| u32::try_from(n).unwrap_or(0);
    (size(tw), size(th), out)
}

impl Stored {
    /// `glyph`, drawn `scale` times larger, stored for orientation `o`; at
    /// 1 bit a pixel when `bits` allows it and every pixel is 0 or 255.
    pub(crate) fn new(glyph: &Glyph, scale: u32, o: Orientation, bits: bool) -> Stored {
        let (format, bpp) = match glyph.format() {
            FontFormat::Mask => (Format::Mask, 1),
            FontFormat::Rgba => (Format::Rgba, 4),
        };
        Stored::from_pixels(
            glyph.pixels(),
            glyph.width(),
            glyph.height(),
            scale,
            format,
            bpp,
            o,
            bits,
        )
    }

    /// A coverage mask `width` by `height` drawn at its size, as the
    /// decoration sprites are.
    pub(crate) fn mask(
        pixels: &[u8],
        width: u32,
        height: u32,
        o: Orientation,
        bits: bool,
    ) -> Stored {
        Stored::from_pixels(pixels, width, height, 1, Format::Mask, 1, o, bits)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the two constructors' shared body"
    )]
    fn from_pixels(
        pixels: &[u8],
        width: u32,
        height: u32,
        scale: u32,
        format: Format,
        bpp: usize,
        o: Orientation,
        bits: bool,
    ) -> Stored {
        let (width, height, data) = turn(pixels, width, height, bpp, o);
        let w = width as usize;
        let lossless = format == Format::Mask && data.iter().all(|&a| a == 0 || a == 255);
        if bits && lossless {
            let stride = w.div_ceil(8);
            let mut packed = vec![0u8; stride.saturating_mul(height as usize)];
            for (dst, src) in packed
                .chunks_exact_mut(stride.max(1))
                .zip(data.chunks_exact(w.max(1)))
            {
                for (x, &a) in src.iter().enumerate() {
                    if let Some(byte) = dst.get_mut(x / 8) {
                        *byte |= (a & 0x80) >> (x % 8);
                    }
                }
            }
            return Stored {
                width,
                height,
                scale: scale.max(1),
                format: Format::Bits,
                stride,
                data: packed,
            };
        }
        Stored {
            width,
            height,
            scale: scale.max(1),
            format,
            stride: w.saturating_mul(bpp),
            data,
        }
    }

    /// Bytes its pixels take: what the cache's byte ceilings count.
    pub(crate) fn bytes(&self) -> usize {
        self.data.len()
    }

    pub(crate) fn is_rgba(&self) -> bool {
        self.format == Format::Rgba
    }

    #[cfg(test)]
    pub(crate) fn format(&self) -> Format {
        self.format
    }

    /// The stored row drawn at line `y`, and the drawn width; `None` above
    /// or below it.
    fn row(&self, y: i64) -> Option<(&[u8], i64)> {
        let s = i64::from(self.scale);
        let drawn_h = i64::from(self.height).saturating_mul(s);
        if y < 0 || y >= drawn_h {
            return None;
        }
        let r = usize::try_from(y.checked_div(s)?).ok()?;
        let start = r.checked_mul(self.stride)?;
        let row = self.data.get(start..start.checked_add(self.stride)?)?;
        Some((row, i64::from(self.width).saturating_mul(s)))
    }

    /// For each output pixel, the stored column under it and that pixel:
    /// `out[i]` is drawn column `x0 + i`; pixels outside are left alone.
    fn columns<T>(&self, x0: i64, drawn_w: i64, out: &mut [T], mut put: impl FnMut(usize, &mut T)) {
        let skip = usize::try_from(0i64.saturating_sub(x0)).unwrap_or(0);
        let first = x0.max(0);
        let s = usize::try_from(self.scale).unwrap_or(1).max(1);
        let Ok(first) = usize::try_from(first) else {
            return;
        };
        let n = usize::try_from(drawn_w.saturating_sub(x0.max(0))).unwrap_or(0);
        let mut col = first.checked_div(s).unwrap_or(0);
        let mut left = s.wrapping_sub(first.checked_rem(s).unwrap_or(0));
        for o in out.iter_mut().skip(skip).take(n) {
            put(col, o);
            left = left.wrapping_sub(1);
            if left == 0 {
                col = col.wrapping_add(1);
                left = s;
            }
        }
    }

    /// The coverage along drawn line `y` from drawn column `x0`, one byte
    /// for each of `out`, 0 outside the glyph. A colour glyph's coverage
    /// is its alpha.
    pub(crate) fn mask_line(&self, x0: i64, y: i64, out: &mut [u8]) {
        out.fill(0);
        let Some((row, drawn_w)) = self.row(y) else {
            return;
        };
        match self.format {
            Format::Bits => self.columns(x0, drawn_w, out, |c, o| {
                let bit = row.get(c / 8).map_or(0, |b| b << (c % 8) & 0x80);
                *o = if bit != 0 { 255 } else { 0 };
            }),
            Format::Mask => self.columns(x0, drawn_w, out, |c, o| {
                *o = row.get(c).copied().unwrap_or(0)
            }),
            Format::Rgba => self.columns(x0, drawn_w, out, |c, o| {
                *o = row
                    .get(c.wrapping_mul(4).wrapping_add(3))
                    .copied()
                    .unwrap_or(0);
            }),
        }
    }

    /// The premultiplied `0xAARRGGBB` pixels along drawn line `y` from drawn
    /// column `x0`, 0 outside the glyph. A mask is white at its coverage.
    pub(crate) fn argb_line(&self, x0: i64, y: i64, out: &mut [u32]) {
        out.fill(0);
        let Some((row, drawn_w)) = self.row(y) else {
            return;
        };
        if self.format != Format::Rgba {
            return;
        }
        self.columns(x0, drawn_w, out, |c, o| {
            let at = c.wrapping_mul(4);
            if let Some(&[r, g, b, a]) = row.get(at..at.wrapping_add(4)) {
                *o = u32::from(a) << 24 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
            }
        });
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn glyph(w: u32, h: u32, f: impl Fn(u32, u32) -> u8) -> Glyph {
        let px = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| f(x, y))
            .collect();
        Glyph::from_pixels(w, h, FontFormat::Mask, px).unwrap()
    }

    fn drawn(s: &Stored, w: usize, h: i64) -> Vec<Vec<u8>> {
        (0..h)
            .map(|y| {
                let mut line = vec![0; w];
                s.mask_line(0, y, &mut line);
                line
            })
            .collect()
    }

    #[test]
    fn a_two_level_mask_is_stored_at_one_bit() {
        let g = glyph(11, 3, |x, y| if (x + y) % 3 == 0 { 255 } else { 0 });
        let s = Stored::new(&g, 1, Orientation::Normal, true);
        assert_eq!((s.format(), s.bytes()), (Format::Bits, 6));
        let eight = Stored::new(&g, 1, Orientation::Normal, false);
        assert_eq!((eight.format(), eight.bytes()), (Format::Mask, 33));
        assert_eq!(drawn(&s, 11, 3), drawn(&eight, 11, 3));
        let grey = glyph(2, 2, |x, _| if x == 0 { 128 } else { 255 });
        assert_eq!(
            Stored::new(&grey, 1, Orientation::Normal, true).format(),
            Format::Mask
        );
    }

    #[test]
    fn a_stored_glyph_is_drawn_scaled_and_clipped() {
        let g = glyph(2, 1, |x, _| if x == 0 { 255 } else { 7 });
        let s = Stored::new(&g, 3, Orientation::Normal, true);
        let mut line = [9u8; 8];
        s.mask_line(-1, 2, &mut line);
        assert_eq!(line, [0, 255, 255, 255, 7, 7, 7, 0]);
        s.mask_line(4, 0, &mut line);
        assert_eq!(line, [7, 7, 0, 0, 0, 0, 0, 0]);
        s.mask_line(0, 3, &mut line);
        assert_eq!(line, [0; 8]);
    }

    // picture.c:235-269: each orientation as the C reference turned pixels
    #[test]
    fn turning_follows_the_c_reference() {
        let g = glyph(3, 2, |x, y| u8::try_from(1 + x + 3 * y).unwrap());
        let rows = |o| {
            drawn(
                &Stored::new(&g, 1, o, true),
                if o == Orientation::Right || o == Orientation::Left {
                    2
                } else {
                    3
                },
                if o == Orientation::Right || o == Orientation::Left {
                    3
                } else {
                    2
                },
            )
        };
        assert_eq!(rows(Orientation::Normal), [vec![1, 2, 3], vec![4, 5, 6]]);
        assert_eq!(
            rows(Orientation::UpsideDown),
            [vec![6, 5, 4], vec![3, 2, 1]]
        );
        assert_eq!(
            rows(Orientation::Right),
            [vec![4, 1], vec![5, 2], vec![6, 3]]
        );
        assert_eq!(
            rows(Orientation::Left),
            [vec![3, 6], vec![2, 5], vec![1, 4]]
        );
    }

    #[test]
    fn a_colour_glyph_keeps_its_pixels() {
        let g = Glyph::from_pixels(1, 1, FontFormat::Rgba, vec![10, 20, 30, 40]).unwrap();
        let s = Stored::new(&g, 2, Orientation::Normal, true);
        assert!(s.is_rgba());
        let mut line = [0u32; 3];
        s.argb_line(0, 1, &mut line);
        assert_eq!(line, [0x280a_141e, 0x280a_141e, 0]);
        let mut cov = [0u8; 3];
        s.mask_line(1, 0, &mut cov);
        assert_eq!(cov, [40, 0, 0]);
    }
}
