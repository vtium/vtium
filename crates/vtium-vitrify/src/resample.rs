//! Scaling a color bitmap down to its cells (ADR-1200 D13): an emoji
//! font's PNG strikes, Noto Color Emoji's at 109 pixels to the em, drawn
//! at 8 to 48. The operator chose Lanczos followed by a light sharpen,
//! in premultiplied color (vtium/vtium#23, 2026-10-28), from a comparison
//! of six ways at 16 and 8 pixels; the others stay here, one chosen when
//! the tool is built:
//!
//! ```text
//! VTIUM_VITRIFY_COLOR_SCALE=lanczos-sharpen   the default
//!                           lanczos           Lanczos, a = 3
//!                           box               the area each pixel covers, averaged
//!                           box-snap          box, then alpha snapped at half
//!                           nearest           the nearest source pixel
//! ```
//!
//! A build-time setting, not Cargo features: the methods exclude each
//! other, and features are additive, so two crates of a workspace could
//! turn on two at once. An unknown value fails the build.
//!
//! Every method works over premultiplied RGBA. Weights, sizes and
//! placement are computed in f64 with IEEE basic operations alone
//! (addition, multiplication, division, floor, ceil, round) and rounded to
//! 14-bit integers; pixels are integers. So the same glyph gives the same
//! bytes on every host: no `sin`, `exp` or `powf` from the platform's
//! maths library.

/// A way to scale a color bitmap.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    /// Lanczos, then an unsharp mask of radius 0.8 at 120 per cent.
    LanczosSharpen,
    Lanczos,
    Box,
    /// Box, then every pixel opaque or clear at half alpha.
    BoxSnap,
    Nearest,
}

impl Method {
    pub const ALL: [Method; 5] = [
        Method::LanczosSharpen,
        Method::Lanczos,
        Method::Box,
        Method::BoxSnap,
        Method::Nearest,
    ];

    /// Its name, as `VTIUM_VITRIFY_COLOR_SCALE` and `META` spell it.
    pub const fn name(self) -> &'static str {
        match self {
            Method::LanczosSharpen => "lanczos-sharpen",
            Method::Lanczos => "lanczos",
            Method::Box => "box",
            Method::BoxSnap => "box-snap",
            Method::Nearest => "nearest",
        }
    }
}

const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        #[expect(clippy::indexing_slicing, reason = "i < a.len() == b.len()")]
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

#[expect(
    clippy::panic,
    reason = "a const panic is the build failing on an unknown setting"
)]
const fn chosen(setting: Option<&str>) -> Method {
    let Some(s) = setting else {
        return Method::LanczosSharpen;
    };
    let mut i = 0;
    while i < Method::ALL.len() {
        #[expect(clippy::indexing_slicing, reason = "i < ALL.len()")]
        let m = Method::ALL[i];
        if same(s, m.name()) {
            return m;
        }
        i += 1;
    }
    panic!("VTIUM_VITRIFY_COLOR_SCALE is none of lanczos-sharpen, lanczos, box, box-snap, nearest")
}

/// The method this build scales color bitmaps with.
pub const METHOD: Method = chosen(option_env!("VTIUM_VITRIFY_COLOR_SCALE"));

/// A premultiplied RGBA image, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y as usize) * (self.width as usize) + x as usize) * 4;
        self.pixels
            .get(i..i + 4)
            .and_then(|p| p.try_into().ok())
            .unwrap_or([0; 4])
    }
}

/// `src` scaled to `width` by `height` with `method`; every color byte
/// at most its alpha.
pub fn resize(src: &Image, width: u32, height: u32, method: Method) -> Image {
    let (width, height) = (width.max(1), height.max(1));
    let mut out = match method {
        Method::Nearest => nearest(src, width, height),
        Method::Box => area(src, width, height),
        Method::BoxSnap => snap(area(src, width, height)),
        Method::Lanczos => lanczos(src, width, height),
        Method::LanczosSharpen => sharpen(lanczos(src, width, height)),
    };
    for p in out.pixels.chunks_exact_mut(4) {
        if let [r, g, b, a] = p {
            (*r, *g, *b) = ((*r).min(*a), (*g).min(*a), (*b).min(*a));
        }
    }
    out
}

fn nearest(src: &Image, width: u32, height: u32) -> Image {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let sy = ((2 * u64::from(y) + 1) * u64::from(src.height) / (2 * u64::from(height))) as u32;
        for x in 0..width {
            let sx =
                ((2 * u64::from(x) + 1) * u64::from(src.width) / (2 * u64::from(width))) as u32;
            pixels.extend(src.at(sx, sy));
        }
    }
    Image {
        width,
        height,
        pixels,
    }
}

/// How much of output pixel `o`'s span, of `n` out of `m` source pixels,
/// source pixel `i` covers, in units of 1 / `n`: the spans are `[i * n,
/// (i + 1) * n)` and `[o * m, (o + 1) * m)`.
fn overlap(i: u64, o: u64, n: u64, m: u64) -> u64 {
    let lo = (i * n).max(o * m);
    let hi = ((i + 1) * n).min((o + 1) * m);
    hi.saturating_sub(lo)
}

/// Each output pixel the average of the source it covers, weighted by how
/// much of each source pixel it covers, exactly.
fn area(src: &Image, width: u32, height: u32) -> Image {
    let (sw, sh, dw, dh) = (
        u64::from(src.width),
        u64::from(src.height),
        u64::from(width),
        u64::from(height),
    );
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for oy in 0..dh {
        let ys = (oy * sh / dh)..((oy + 1) * sh).div_ceil(dh).min(sh);
        for ox in 0..dw {
            let xs = (ox * sw / dw)..((ox + 1) * sw).div_ceil(dw).min(sw);
            let mut acc = [0u64; 4];
            for y in ys.clone() {
                let wy = overlap(y, oy, dh, sh);
                for x in xs.clone() {
                    let w = wy * overlap(x, ox, dw, sw);
                    for (a, c) in acc.iter_mut().zip(src.at(x as u32, y as u32)) {
                        *a += w * u64::from(c);
                    }
                }
            }
            let total = sw * sh;
            pixels.extend(acc.map(|a| ((a + total / 2) / total).min(255) as u8));
        }
    }
    Image {
        width,
        height,
        pixels,
    }
}

/// Every pixel opaque at half alpha or more, its colors divided by its
/// alpha; clear below.
fn snap(mut image: Image) -> Image {
    for p in image.pixels.chunks_exact_mut(4) {
        if let [r, g, b, a] = p {
            if *a >= 128 {
                let full = |c: u8| {
                    ((u32::from(c) * 255 + u32::from(*a) / 2) / u32::from(*a)).min(255) as u8
                };
                (*r, *g, *b, *a) = (full(*r), full(*g), full(*b), 255);
            } else {
                (*r, *g, *b, *a) = (0, 0, 0, 0);
            }
        }
    }
    image
}

/// The fixed point of the filters' weights: 1.0 is `1 << WEIGHT_BITS`.
const WEIGHT_BITS: u32 = 14;

/// `sin(pi * t)`, from its Taylor series with arithmetic alone.
fn sin_pi(t: f64) -> f64 {
    // to [-1, 1), then by symmetry to [-0.5, 0.5]
    let mut t = t - 2.0 * (t / 2.0 + 0.5).floor();
    if t > 0.5 {
        t = 1.0 - t;
    } else if t < -0.5 {
        t = -1.0 - t;
    }
    let x = core::f64::consts::PI * t;
    let x2 = x * x;
    let mut term = x;
    let mut sum = x;
    for k in 1..12u32 {
        let k = f64::from(k);
        term = -term * x2 / ((2.0 * k) * (2.0 * k + 1.0));
        sum += term;
    }
    sum
}

/// The Lanczos kernel of three lobes.
fn lanczos3(t: f64) -> f64 {
    if t == 0.0 {
        return 1.0;
    }
    if t.abs() >= 3.0 {
        return 0.0;
    }
    let pi2 = core::f64::consts::PI * core::f64::consts::PI;
    3.0 * sin_pi(t) * sin_pi(t / 3.0) / (pi2 * t * t)
}

/// For each of `out` pixels from `n` source pixels, its first source
/// pixel and its weights, which sum to `1 << WEIGHT_BITS`; the kernel
/// widened by the reduction when scaling down, as Pillow widens it.
fn weights(n: u32, out: u32) -> Vec<(u32, Vec<i64>)> {
    let scale = f64::from(n) / f64::from(out);
    let wide = scale.max(1.0);
    let support = 3.0 * wide;
    (0..out)
        .map(|o| {
            let centre = (f64::from(o) + 0.5) * scale;
            let first = (centre - support).floor().max(0.0) as u32;
            let last = ((centre + support).ceil() as u32).min(n);
            let raw: Vec<f64> = (first..last)
                .map(|i| lanczos3((f64::from(i) + 0.5 - centre) / wide))
                .collect();
            let total: f64 = raw.iter().sum();
            let one = f64::from(1u32 << WEIGHT_BITS);
            let mut w: Vec<i64> = raw
                .iter()
                .map(|r| (r / total * one).round() as i64)
                .collect();
            // the rounding's remainder goes to the largest weight
            let rest = (1i64 << WEIGHT_BITS) - w.iter().sum::<i64>();
            if let Some(m) = w.iter_mut().max_by_key(|v| **v) {
                *m += rest;
            }
            (first, w)
        })
        .collect()
}

/// Lanczos in two passes, across then down, rounded once at the end.
fn lanczos(src: &Image, width: u32, height: u32) -> Image {
    let across = weights(src.width, width);
    let down = weights(src.height, height);
    // each row scaled across, kept at the weights' precision
    let mut rows: Vec<[i64; 4]> = Vec::with_capacity((width * src.height) as usize);
    for y in 0..src.height {
        for (first, w) in &across {
            let mut acc = [0i64; 4];
            for (k, wk) in w.iter().enumerate() {
                for (a, c) in acc.iter_mut().zip(src.at(first + k as u32, y)) {
                    *a += wk * i64::from(c);
                }
            }
            rows.push(acc);
        }
    }
    let shift = 2 * WEIGHT_BITS;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for (first, w) in &down {
        for x in 0..width {
            let mut acc = [0i64; 4];
            for (k, wk) in w.iter().enumerate() {
                let i = ((first + k as u32) * width + x) as usize;
                if let Some(row) = rows.get(i) {
                    for (a, c) in acc.iter_mut().zip(row) {
                        *a += wk * c;
                    }
                }
            }
            pixels.extend(acc.map(|a| ((a + (1 << (shift - 1))) >> shift).clamp(0, 255) as u8));
        }
    }
    Image {
        width,
        height,
        pixels,
    }
}

/// A Gaussian of radius 0.8 over five taps, in units of 1 / 4096:
/// `exp(-x^2 / 1.28)` at 0, 1 and 2, normalised.
const BLUR: [i64; 5] = [90, 936, 2044, 936, 90];

/// An unsharp mask of radius 0.8 at 120 per cent with no threshold, the
/// sharpen the operator chose: each channel, alpha included, moved away
/// from its blur by 1.2 times the difference.
fn sharpen(image: Image) -> Image {
    let (w, h) = (image.width as i64, image.height as i64);
    let at = |x: i64, y: i64| image.at(x.clamp(0, w - 1) as u32, y.clamp(0, h - 1) as u32);
    let mut across: Vec<[i64; 4]> = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0i64; 4];
            for (k, wk) in BLUR.iter().enumerate() {
                for (a, c) in acc.iter_mut().zip(at(x + k as i64 - 2, y)) {
                    *a += wk * i64::from(c);
                }
            }
            across.push(acc);
        }
    }
    let mut pixels = Vec::with_capacity(image.pixels.len());
    for y in 0..h {
        for x in 0..w {
            let mut blur = [0i64; 4];
            for (k, wk) in BLUR.iter().enumerate() {
                let yy = (y + k as i64 - 2).clamp(0, h - 1);
                if let Some(row) = across.get((yy * w + x) as usize) {
                    for (b, c) in blur.iter_mut().zip(row) {
                        *b += wk * c;
                    }
                }
            }
            let px = image.at(x as u32, y as u32);
            for (c, b) in px.iter().zip(blur) {
                // in + 1.2 (in - blur), the blur at 1 / 2^24
                let diff = (i64::from(*c) << 24) - b;
                let v = i64::from(*c) + (diff * 6 / 5 + (1 << 23)).div_euclid(1 << 24);
                pixels.push(v.clamp(0, 255) as u8);
            }
        }
    }
    Image {
        width: image.width,
        height: image.height,
        pixels,
    }
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, p: [u8; 4]) -> Image {
        Image {
            width: w,
            height: h,
            pixels: p.repeat((w * h) as usize),
        }
    }

    /// A 12x12 image: an opaque red square in its middle 6x6, clear around.
    fn square() -> Image {
        let mut img = flat(12, 12, [0; 4]);
        for y in 3..9 {
            for x in 3..9 {
                let i = (y * 12 + x) * 4;
                img.pixels[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        img
    }

    #[test]
    fn the_default_is_lanczos_then_a_sharpen() {
        assert_eq!(chosen(None), Method::LanczosSharpen);
        for m in Method::ALL {
            assert_eq!(chosen(Some(m.name())), m);
        }
    }

    #[test]
    fn a_flat_image_stays_flat_whatever_the_method() {
        let p = [60, 30, 10, 200];
        for m in Method::ALL {
            let want = if m == Method::BoxSnap {
                [77, 38, 13, 255]
            } else {
                p
            };
            let out = resize(&flat(9, 7, p), 4, 3, m);
            assert_eq!((out.width, out.height), (4, 3), "{m:?}");
            assert!(
                out.pixels.chunks(4).all(|q| q == want),
                "{m:?}: {:?}",
                &out.pixels[..8]
            );
        }
    }

    #[test]
    fn box_averages_the_area_each_pixel_covers() {
        // 12 to 5: output pixel (1, 1) spans source 2.4 to 4.8 each way,
        // the square 1.8 of the 2.4, so 81 / 144 of its area
        let out = resize(&square(), 5, 5, Method::Box);
        assert_eq!(&out.pixels[(5 + 1) * 4..(5 + 1) * 4 + 4], &[143, 0, 0, 143]);
        assert_eq!(&out.pixels[0..4], &[0; 4]);
        let snapped = resize(&square(), 5, 5, Method::BoxSnap);
        assert_eq!(
            &snapped.pixels[(5 + 1) * 4..(5 + 1) * 4 + 4],
            &[255, 0, 0, 255]
        );
        // 12 to 3: every block is wholly square or wholly clear but the
        // middle's edges
        let n = resize(&square(), 3, 3, Method::Nearest);
        assert_eq!(&n.pixels[(3 + 1) * 4..(3 + 1) * 4 + 4], &[255, 0, 0, 255]);
        assert_eq!(&n.pixels[0..4], &[0; 4]);
    }

    #[test]
    fn lanczos_keeps_premultiplied_color_and_the_sharpen_raises_contrast() {
        let soft = resize(&square(), 6, 6, Method::Lanczos);
        let sharp = resize(&square(), 6, 6, Method::LanczosSharpen);
        for img in [&soft, &sharp] {
            for p in img.pixels.chunks(4) {
                assert!(p[0] <= p[3] && p[1] == 0 && p[2] == 0, "{p:?}");
            }
        }
        // the square's edge at 6 pixels falls between source pixels: the
        // sharpen pushes its inside up and its outside down
        let alpha = |img: &Image, x: usize| img.pixels[(2 * 6 + x) * 4 + 3];
        assert!(alpha(&sharp, 2) >= alpha(&soft, 2));
        assert!(alpha(&sharp, 0) <= alpha(&soft, 0));
        assert!(alpha(&soft, 2) > 200, "{}", alpha(&soft, 2));
    }

    #[test]
    fn sin_pi_is_sin_of_pi_t() {
        for i in -30..=30 {
            let t = f64::from(i) / 10.0;
            let d = sin_pi(t) - (core::f64::consts::PI * t).sin();
            assert!(d.abs() < 1e-12, "{t}: {d}");
        }
    }
}
