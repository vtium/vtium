//! Scaling and turning a picture in integer arithmetic only, so a picture
//! scales to the same pixels on every machine (docs/architecture.md,
//! "Determinism").
//!
//! The filter is the C reference's (kmscon `src/render/picture.c`): each
//! output pixel is a weighted sum of source pixels, with one list of
//! weights for each output column and one for each output row. A weight is
//! 16-bit fixed point, and each list sums to exactly 65,536, so an opaque
//! picture stays opaque and a premultiplied channel never passes its alpha.
//! Where a picture shrinks, each output pixel averages the source pixels
//! its span covers, weighted by how much of each it covers (a box filter);
//! where it grows, it blends the two source pixels nearest its centre
//! (bilinear). Each output pixel is computed directly from its weights and
//! rounded once, which gives the bytes the C reference's two passes gave,
//! whether the pixel goes into a kept copy or is scaled for one frame.

use crate::target::Orientation;

/// One, in the weights' fixed point.
const ONE: u32 = 1 << 16;

/// Half of one after both axes' weights multiply, for rounding.
const HALF: u64 = 1 << 31;

/// The source pixels and weights of the output pixels `o0` onward along
/// one axis: output `o` reads `count` source pixels from `first`.
#[derive(Clone, Debug, Default)]
pub(crate) struct Taps {
    o0: u32,
    first: Vec<u32>,
    count: Vec<u32>,
    /// `max` weights for each output pixel.
    w: Vec<u32>,
    max: usize,
}

impl Taps {
    /// The weights that take `s` source pixels to `d` output pixels, for
    /// the outputs from `o0` up to `o1`. Both sizes are at least 1 and
    /// `o1` is at most `d`; anything else gives no outputs.
    pub(crate) fn new(s: u32, d: u32, o0: u32, o1: u32) -> Taps {
        let o1 = o1.min(d);
        if s == 0 || d == 0 || o0 >= o1 {
            return Taps::default();
        }
        let max = if s > d {
            usize::try_from(s.checked_div(d).unwrap_or(0))
                .unwrap_or(usize::MAX)
                .saturating_add(2)
        } else {
            2
        };
        let n = usize::try_from(o1.saturating_sub(o0)).unwrap_or(0);
        let mut t = Taps {
            o0,
            first: Vec::with_capacity(n),
            count: Vec::with_capacity(n),
            w: vec![0; n.saturating_mul(max)],
            max,
        };
        for (i, o) in (o0..o1).enumerate() {
            let at = i.saturating_mul(max);
            let w = t.w.get_mut(at..at.saturating_add(max)).unwrap_or_default();
            let (first, count) = weights(s, d, o, w);
            t.first.push(first);
            t.count.push(count);
        }
        t
    }

    /// Output `o`'s first source pixel and its weights.
    fn at(&self, o: u32) -> Option<(u32, &[u32])> {
        let i = usize::try_from(o.checked_sub(self.o0)?).ok()?;
        let first = *self.first.get(i)?;
        let count = usize::try_from(*self.count.get(i)?).ok()?;
        let at = i.checked_mul(self.max)?;
        Some((first, self.w.get(at..at.checked_add(count)?)?))
    }
}

/// Output `o`'s weights along an axis of `s` source pixels drawn as `d`,
/// into `w`: its first source pixel and how many it reads.
fn weights(s: u32, d: u32, o: u32, w: &mut [u32]) -> (u32, u32) {
    let set = |w: &mut [u32], k: usize, v: u32| {
        if let Some(x) = w.get_mut(k) {
            *x = v;
        }
    };
    if s == d {
        set(w, 0, ONE);
        return (o, 1);
    }
    if s > d {
        // the box: the source span [o * s, (o + 1) * s), in units where a
        // source pixel is d wide
        let (s64, d64) = (u64::from(s), u64::from(d));
        let lo = u64::from(o).saturating_mul(s64);
        let hi = lo.saturating_add(s64);
        let first = u32::try_from(lo.checked_div(d64).unwrap_or(0)).unwrap_or(u32::MAX);
        let (mut count, mut sum, mut big) = (0usize, 0u32, 0usize);
        let mut i = first;
        while i < s && u64::from(i).saturating_mul(d64) < hi && count < w.len() {
            let a = u64::from(i).saturating_mul(d64);
            let b = a.saturating_add(d64);
            let ov = hi.min(b).saturating_sub(lo.max(a));
            let v = u32::try_from(
                ov.saturating_mul(u64::from(ONE))
                    .checked_div(s64)
                    .unwrap_or(0),
            )
            .unwrap_or(ONE);
            set(w, count, v);
            sum = sum.saturating_add(v);
            if v > w.get(big).copied().unwrap_or(0) {
                big = count;
            }
            count = count.saturating_add(1);
            i = i.saturating_add(1);
        }
        // the rounding left over goes to the heaviest weight
        if let Some(x) = w.get_mut(big) {
            *x = x.saturating_add(ONE.saturating_sub(sum));
        }
        return (first, u32::try_from(count).unwrap_or(0));
    }
    // bilinear: the output pixel's centre, in source pixels
    let num = u128::from(o)
        .saturating_mul(2)
        .saturating_add(1)
        .saturating_mul(u128::from(s))
        .saturating_mul(u128::from(ONE));
    let centre = num
        .checked_div(u128::from(d).saturating_mul(2))
        .unwrap_or(0);
    let p = i128::try_from(centre)
        .unwrap_or(i128::MAX)
        .saturating_sub(i128::from(ONE / 2))
        .max(0);
    let one = i128::from(ONE);
    let mut i = u32::try_from(p.checked_div(one).unwrap_or(0)).unwrap_or(u32::MAX);
    let mut f = u32::try_from(p.checked_rem(one).unwrap_or(0)).unwrap_or(0);
    if i >= s.saturating_sub(1) {
        i = s.saturating_sub(1);
        f = 0;
    }
    set(w, 0, ONE.saturating_sub(f));
    set(w, 1, f);
    (i, if f == 0 { 1 } else { 2 })
}

/// One channel, `c` 0 to 3 from blue up, of a pixel.
fn chan(p: u32, c: u32) -> u64 {
    u64::from(p >> c.saturating_mul(8) & 0xff)
}

/// The source of a scaled picture: the image's pixels, `stride` a row, and
/// the source rectangle's top left.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Source<'a> {
    pub(crate) pixels: &'a [u32],
    pub(crate) stride: usize,
    pub(crate) x: u32,
    pub(crate) y: u32,
}

impl Source<'_> {
    fn pixel(&self, x: u32, y: u32) -> u32 {
        let x = usize::try_from(self.x.saturating_add(x)).unwrap_or(usize::MAX);
        let y = usize::try_from(self.y.saturating_add(y)).unwrap_or(usize::MAX);
        y.checked_mul(self.stride)
            .and_then(|r| r.checked_add(x))
            .and_then(|i| self.pixels.get(i))
            .copied()
            .unwrap_or(0)
    }
}

/// The scaled picture's pixel at column `u`, row `v`.
pub(crate) fn eval(src: &Source<'_>, tx: &Taps, ty: &Taps, u: u32, v: u32) -> u32 {
    let (Some((fx, wx)), Some((fy, wy))) = (tx.at(u), ty.at(v)) else {
        return 0;
    };
    if let ([ONE], [ONE]) = (wx, wy) {
        return src.pixel(fx, fy);
    }
    let mut acc = [0u64; 4];
    for (k, &wyk) in (0u32..).zip(wy) {
        let row = fy.saturating_add(k);
        let mut h = [0u64; 4];
        for (i, &wxi) in (0u32..).zip(wx) {
            let p = src.pixel(fx.saturating_add(i), row);
            for (c, hc) in (0u32..).zip(h.iter_mut()) {
                *hc = hc.saturating_add(u64::from(wxi).saturating_mul(chan(p, c)));
            }
        }
        for (a, hc) in acc.iter_mut().zip(h) {
            *a = a.saturating_add(u64::from(wyk).saturating_mul(hc));
        }
    }
    let mut out = 0u32;
    for (c, a) in (0u32..).zip(acc) {
        let v = u32::try_from((a.saturating_add(HALF) >> 32).min(255)).unwrap_or(255);
        out |= v << c.saturating_mul(8);
    }
    out
}

/// The picture's column and row under the pixel `ox`, `oy` of its display
/// rectangle, for a picture `w` by `h` turned as `o` says. A quarter turn
/// right puts the picture's row `v` in display column `h - 1 - v`, and a
/// quarter turn left its column `u` in display row `w - 1 - u`, as the
/// renderer turns the grid.
pub(crate) fn uv(o: Orientation, (w, h): (u32, u32), (ox, oy): (u32, u32)) -> (u32, u32) {
    let back = |n: u32, v: u32| n.saturating_sub(1).saturating_sub(v);
    match o {
        Orientation::Normal => (ox, oy),
        Orientation::UpsideDown => (back(w, ox), back(h, oy)),
        Orientation::Right => (oy, back(h, ox)),
        Orientation::Left => (back(w, oy), ox),
    }
}

/// The source rectangle `sw` by `sh` of `src` scaled to `w` by `h` and
/// turned as `o` says, row by row in display order.
pub(crate) fn turned_copy(
    src: &Source<'_>,
    (sw, sh): (u32, u32),
    (w, h): (u32, u32),
    o: Orientation,
) -> Vec<u32> {
    let tx = Taps::new(sw, w, 0, w);
    let ty = Taps::new(sh, h, 0, h);
    let (dw, dh) = if o.turned() { (h, w) } else { (w, h) };
    let n = usize::try_from(u64::from(dw).saturating_mul(u64::from(dh))).unwrap_or(0);
    let mut out = Vec::with_capacity(n);
    for oy in 0..dh {
        for ox in 0..dw {
            let (u, v) = uv(o, (w, h), (ox, oy));
            out.push(eval(src, &tx, &ty, u, v));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sums(t: &Taps, n: u32) {
        for o in 0..n {
            let (_, w) = t.at(o).unwrap_or((0, &[]));
            assert_eq!(w.iter().sum::<u32>(), ONE, "output {o}");
        }
    }

    // picture.c: each output pixel's weights sum to exactly one
    #[test]
    fn every_output_pixels_weights_sum_to_one() {
        for (s, d) in [
            (1, 1),
            (3, 7),
            (7, 3),
            (16, 8),
            (100, 37),
            (37, 100),
            (65535, 1),
        ] {
            sums(&Taps::new(s, d, 0, d), d);
        }
    }

    #[test]
    fn an_opaque_picture_stays_opaque_at_every_scale() {
        let px: Vec<u32> = (0..35u32)
            .map(|i| 0xff00_0000 | (i * 0x0007_0503))
            .collect();
        let src = Source {
            pixels: &px,
            stride: 7,
            x: 0,
            y: 0,
        };
        for (w, h) in [(3, 2), (7, 5), (13, 11), (1, 1)] {
            for p in turned_copy(&src, (7, 5), (w, h), Orientation::Normal) {
                assert_eq!(p >> 24, 0xff, "{w}x{h}: {p:08x}");
            }
        }
    }

    #[test]
    fn at_its_own_size_a_picture_is_its_pixels() {
        let px: Vec<u32> = (0..12u32).map(|i| i * 0x0101_0101).collect();
        let src = Source {
            pixels: &px,
            stride: 4,
            x: 0,
            y: 0,
        };
        assert_eq!(turned_copy(&src, (4, 3), (4, 3), Orientation::Normal), px);
    }

    // a 2x1 picture, a over b, turned each way
    #[test]
    fn a_turned_copy_puts_each_pixel_where_the_grid_turns_it() {
        let (a, b) = (0xff00_0001, 0xff00_0002);
        let px = [a, b];
        let src = Source {
            pixels: &px,
            stride: 2,
            x: 0,
            y: 0,
        };
        let copy = |o| turned_copy(&src, (2, 1), (2, 1), o);
        assert_eq!(copy(Orientation::Normal), [a, b]);
        assert_eq!(copy(Orientation::UpsideDown), [b, a]);
        // a quarter right: the picture's left goes to the top
        assert_eq!(copy(Orientation::Right), [a, b]);
        // a quarter left: the picture's left goes to the bottom
        assert_eq!(copy(Orientation::Left), [b, a]);
    }

    #[test]
    fn doubling_blends_neighbours_and_keeps_the_ends() {
        let px = [0xff00_0000, 0xff00_00ff];
        let src = Source {
            pixels: &px,
            stride: 2,
            x: 0,
            y: 0,
        };
        let out = turned_copy(&src, (2, 1), (4, 1), Orientation::Normal);
        let blue: Vec<u32> = out.iter().map(|p| p & 0xff).collect();
        // centres at -0.25, 0.25, 0.75, 1.25 source pixels
        assert_eq!(blue, [0, 64, 191, 255]);
    }
}
