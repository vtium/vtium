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

/// Pixels to store: `width` by `height` of `format`, `bpp` bytes each.
struct Pixels<'a> {
    data: &'a [u8],
    width: u32,
    height: u32,
    format: Format,
    bpp: usize,
}

/// Lines of pixels in a buffer: each `width` long, `stride` apart from
/// the start of `out`.
#[derive(Debug)]
pub(crate) struct Rows<'a> {
    pub(crate) out: &'a mut [u32],
    pub(crate) stride: usize,
    pub(crate) width: usize,
}

impl<'a> Rows<'a> {
    /// `out` as one line.
    pub(crate) fn one(out: &'a mut [u32]) -> Rows<'a> {
        let width = out.len();
        Rows {
            out,
            stride: width,
            width,
        }
    }
}

/// The pixels a 1-bit glyph draws in, `fg` where a bit is set and `bg`
/// elsewhere, as runs of four pixels for each half byte.
#[derive(Clone, Debug)]
pub(crate) struct Pens {
    colors: (u32, u32),
    nibbles: [[u32; 4]; 16],
}

impl Pens {
    pub(crate) fn new(colors: (u32, u32)) -> Pens {
        let mut p = Pens {
            colors: (0, 0),
            nibbles: [[0; 4]; 16],
        };
        p.set(colors);
        p
    }

    /// Draws in `fg` and `bg` from now on.
    #[inline]
    pub(crate) fn set(&mut self, colors: (u32, u32)) {
        if self.colors == colors {
            return;
        }
        let (fg, bg) = colors;
        let flip = fg ^ bg;
        for (n, run) in (0u8..).zip(self.nibbles.iter_mut()) {
            for (o, bit) in run.iter_mut().zip([8u8, 4, 2, 1]) {
                let set = u32::from(n & bit != 0);
                *o = bg ^ flip & set.wrapping_neg();
            }
        }
        self.colors = colors;
    }

    /// The 8 pixels of `byte`'s bits, from the left.
    #[inline]
    fn eight(&self, byte: u8) -> [u32; 8] {
        let half = |n: u8| {
            self.nibbles
                .get(usize::from(n & 15))
                .copied()
                .unwrap_or_default()
        };
        let ([a, b, c, d], [e, f, g, h]) = (half(byte >> 4), half(byte));
        [a, b, c, d, e, f, g, h]
    }

    /// The pixels of `byte`'s bits from the left, one for each of `out`,
    /// at most 8.
    #[inline]
    fn paint(&self, byte: u8, out: &mut [u32]) {
        for (o, p) in out.iter_mut().zip(self.eight(byte)) {
            *o = p;
        }
    }
}

impl Stored {
    /// `glyph`, drawn `scale` times larger, stored for orientation `o`; at
    /// 1 bit a pixel when `bits` allows it and every pixel is 0 or 255.
    pub(crate) fn new(glyph: &Glyph, scale: u32, o: Orientation, bits: bool) -> Stored {
        let (format, bpp) = match glyph.format() {
            FontFormat::Mask => (Format::Mask, 1),
            FontFormat::Rgba => (Format::Rgba, 4),
        };
        let src = Pixels {
            data: glyph.pixels(),
            width: glyph.width(),
            height: glyph.height(),
            format,
            bpp,
        };
        Stored::from_pixels(src, scale, o, bits)
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
        let src = Pixels {
            data: pixels,
            width,
            height,
            format: Format::Mask,
            bpp: 1,
        };
        Stored::from_pixels(src, 1, o, bits)
    }

    /// The two constructors' shared body.
    fn from_pixels(src: Pixels<'_>, scale: u32, o: Orientation, bits: bool) -> Stored {
        let Pixels {
            data: pixels,
            width,
            height,
            format,
            bpp,
        } = src;
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

    /// Whether it is held at 1 bit a pixel: [`Stored::bits_rows`] draws it.
    pub(crate) fn is_bits(&self) -> bool {
        self.format == Format::Bits
    }

    /// Drawn line `y` of a 1-bit glyph from drawn column `x0`, `fg` where
    /// it is set and `bg` elsewhere, one display pixel for each of `out`:
    /// what [`Stored::mask_line`] and `blend::mask` give, without the
    /// coverage in between.
    pub(crate) fn bits_line(&self, x0: i64, y: i64, colors: (u32, u32), out: &mut [u32]) {
        self.bits_rows(x0, y, &Pens::new(colors), Rows::one(out));
    }

    /// Drawn lines `y0` onward of a 1-bit glyph from drawn column `x0`,
    /// as [`Stored::bits_line`] draws each in `pens`, into `rows`.
    pub(crate) fn bits_rows(&self, x0: i64, y0: i64, pens: &Pens, rows: Rows<'_>) {
        let Rows { out, stride, width } = rows;
        let (fg, bg) = pens.colors;
        let lines = out.chunks_mut(stride.max(1));
        let lines = lines.filter_map(|line| line.get_mut(..width));
        if self.format != Format::Bits {
            lines.for_each(|line| line.fill(bg));
            return;
        }
        let height = i64::from(self.height);
        // where line `y`'s stored bytes start; `None` above or below
        let start = |y: i64| {
            let r = usize::try_from(y).ok().filter(|_| y < height)?;
            r.checked_mul(self.stride)
        };
        if self.scale == 1 && x0 == 0 && width == 8 {
            // minspec's cell, 8 pixels wide at the font's size: a byte a line
            for (y, line) in (y0..).zip(out.chunks_mut(stride.max(1))) {
                let byte = start(y).and_then(|s| self.data.get(s)).copied();
                if let Some(px) = line.first_chunk_mut::<8>() {
                    *px = pens.eight(byte.unwrap_or(0));
                }
            }
            return;
        }
        for (y, line) in (y0..).zip(lines) {
            if self.scale == 1 && x0 == 0 {
                // at the font's size from the glyph's left edge, a byte at a
                // time; the padding past the width is clear
                let row = start(y)
                    .and_then(|s| self.data.get(s..s.saturating_add(self.stride)))
                    .unwrap_or_default();
                let mut bytes = row.iter().copied().chain(core::iter::repeat(0));
                let mut eights = line.chunks_exact_mut(8);
                for (chunk, byte) in eights.by_ref().zip(bytes.by_ref()) {
                    pens.paint(byte, chunk);
                }
                if let Some(byte) = bytes.next() {
                    pens.paint(byte, eights.into_remainder());
                }
                continue;
            }
            match self.row(y) {
                Some((row, drawn_w)) => {
                    line.fill(bg);
                    self.columns(x0, drawn_w, line, |c, o| {
                        if row.get(c / 8).is_some_and(|b| b << (c % 8) & 0x80 != 0) {
                            *o = fg;
                        }
                    });
                }
                None => line.fill(bg),
            }
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

    // the 1-bit line is the coverage line in two colours, at every offset
    // and scale, cut at either edge
    #[test]
    fn a_bits_line_is_its_mask_line_in_two_colours() {
        let g = glyph(11, 3, |x, y| if (x * 7 + y) % 3 == 0 { 255 } else { 0 });
        for scale in 1..=3 {
            let s = Stored::new(&g, scale, Orientation::Normal, true);
            assert!(s.is_bits());
            for y in -1..10 {
                for x0 in -9..40 {
                    for w in [1, 8, 13, 40] {
                        let mut cov = vec![0u8; w];
                        s.mask_line(x0, y, &mut cov);
                        let want: Vec<u32> =
                            cov.iter().map(|&a| if a == 255 { 1 } else { 2 }).collect();
                        let mut got = vec![0u32; w];
                        s.bits_line(x0, y, (1, 2), &mut got);
                        assert_eq!(got, want, "scale {scale}, line {y}, from {x0}, {w} wide");
                    }
                }
            }
        }
    }

    // the table follows each change of colour: the background alone, the
    // foreground alone, both, and back
    #[test]
    fn pens_follow_every_change_of_colour() {
        let mut pens = Pens::new((1, 2));
        for colors in [(1, 3), (4, 3), (5, 6), (5, 2), (1, 2)] {
            pens.set(colors);
            let (fg, bg) = colors;
            assert_eq!(
                pens.eight(0b1010_0110),
                [fg, bg, fg, bg, bg, fg, fg, bg],
                "{colors:?}"
            );
        }
    }

    // lines drawn into a buffer of longer lines are each the coverage line
    // in two colours, and the pixels between them are left alone
    #[test]
    fn bits_rows_are_mask_lines_at_a_stride() {
        for gw in [8, 11] {
            let g = glyph(gw, 5, |x, y| if (x * 5 + y) % 3 == 0 { 255 } else { 0 });
            for scale in 1..=2 {
                let s = Stored::new(&g, scale, Orientation::Normal, true);
                for y0 in -3..12 {
                    for x0 in [-2, 0, 3] {
                        for w in [8, 13] {
                            for n in 1..=6 {
                                let stride = w + 5;
                                let mut buf = vec![7u32; stride * (n - 1) + w];
                                let rows = Rows {
                                    out: &mut buf,
                                    stride,
                                    width: w,
                                };
                                s.bits_rows(x0, y0, &Pens::new((1, 2)), rows);
                                for (y, line) in (y0..).zip(buf.chunks(stride)) {
                                    let mut cov = vec![0u8; w];
                                    s.mask_line(x0, y, &mut cov);
                                    let mut want: Vec<u32> =
                                        cov.iter().map(|&a| if a == 255 { 1 } else { 2 }).collect();
                                    want.extend(core::iter::repeat_n(7, line.len() - w));
                                    let at = format!("{gw} wide at {scale}, {y0}, {x0}, {w}");
                                    assert_eq!(line, &want[..], "{at}, line {y}");
                                }
                            }
                        }
                    }
                }
            }
        }
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
