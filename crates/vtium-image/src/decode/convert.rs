//! The one conversion into the store's premultiplied format (ADR-1101 D7),
//! which the image helper, the console's raw decoder and the replay
//! interface's fixture host all use, so the same pixels store the same
//! bytes whichever way they arrive.
//!
//! Raw pixels come as RGB, 3 bytes a pixel, or as RGBA, 4 bytes a pixel,
//! with straight or premultiplied alpha. [`premultiply`] turns one pixel
//! with straight alpha into the store's pixel, a `u32` laid out as
//! `0xAARRGGBB` with each color already multiplied by the alpha, using
//! integers only: each color becomes `(color × alpha + 127) / 255`, so
//! every machine stores the same pixels. A pixel whose alpha is already
//! premultiplied keeps its colors, each clamped to its alpha.
//!
//! The helper's reply carries the same pixels as bytes, R, G, B, A in that
//! order ([`to_reply`]). The console reads a reply with [`from_reply`],
//! which clamps every color to its alpha and finds in the same pass
//! whether every pixel is opaque, so a helper that sends wrong pixels
//! gives that one picture wrong colors and never a value out of range.

/// How many bytes a raw pixel has, and in which order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// R, G, B: 3 bytes a pixel, always opaque.
    Rgb,
    /// R, G, B, A: 4 bytes a pixel.
    Rgba,
}

impl Layout {
    /// The bytes one pixel takes.
    pub const fn bytes(self) -> usize {
        match self {
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }
}

/// Whether a pixel's colors are already multiplied by its alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alpha {
    /// The colors are as they would show on an opaque pixel, as kitty's
    /// `f=32` sends them.
    Straight,
    /// The colors are already multiplied by the alpha.
    Premultiplied,
}

/// The store's pixel, `0xAARRGGBB` premultiplied, of straight-alpha `r`,
/// `g`, `b` and `a`.
pub fn premultiply(r: u8, g: u8, b: u8, a: u8) -> u32 {
    let a32 = u32::from(a);
    let scale = |c: u8| -> u32 {
        if a == 0xff {
            u32::from(c)
        } else {
            u32::from(c)
                .saturating_mul(a32)
                .saturating_add(127)
                .checked_div(255)
                .unwrap_or(0)
        }
    };
    a32.wrapping_shl(24) | scale(r).wrapping_shl(16) | scale(g).wrapping_shl(8) | scale(b)
}

/// The store's pixel of premultiplied `r`, `g`, `b` and `a`, each color
/// clamped to the alpha.
pub fn clamped(r: u8, g: u8, b: u8, a: u8) -> u32 {
    u32::from(a).wrapping_shl(24)
        | u32::from(r.min(a)).wrapping_shl(16)
        | u32::from(g.min(a)).wrapping_shl(8)
        | u32::from(b.min(a))
}

/// The store's pixel of one raw pixel's `bytes` in `layout` with `alpha`;
/// 0 for a group of the wrong length, which a caller never passes.
pub fn pixel(layout: Layout, alpha: Alpha, bytes: &[u8]) -> u32 {
    match (layout, bytes) {
        (Layout::Rgb, &[r, g, b]) => premultiply(r, g, b, 0xff),
        (Layout::Rgba, &[r, g, b, a]) => match alpha {
            Alpha::Straight => premultiply(r, g, b, a),
            Alpha::Premultiplied => clamped(r, g, b, a),
        },
        _ => 0,
    }
}

/// The store's pixel of palette index `index` with straight alpha `a`,
/// whose color `colors` gives, as kitty's `f=8` and `f=16` send it
/// (ADR-1100, D15).
pub fn indexed(colors: &[[u8; 3]; 256], index: u8, a: u8) -> u32 {
    let [r, g, b] = colors.get(usize::from(index)).copied().unwrap_or([0; 3]);
    premultiply(r, g, b, a)
}

/// The R, G, B, A bytes of the store's pixel `px`, as a reply carries it.
pub fn reply_bytes(px: u32) -> [u8; 4] {
    let [a, r, g, b] = px.to_be_bytes();
    [r, g, b, a]
}

/// The whole pixels of `src`, in `layout` with `alpha`, as the reply's
/// premultiplied R, G, B, A bytes, appended to `out`. A partial pixel at
/// the end is left out. The image helper converts every picture it returns
/// this way, decoded or described.
pub fn to_reply(layout: Layout, alpha: Alpha, src: &[u8], out: &mut Vec<u8>) {
    let n = src.len().checked_div(layout.bytes()).unwrap_or(0);
    out.reserve(n.saturating_mul(4));
    for group in src.chunks_exact(layout.bytes()) {
        out.extend_from_slice(&reply_bytes(pixel(layout, alpha, group)));
    }
}

/// [`to_reply`] in place: the first `pixels` pixels of `data`, in `layout`
/// with `alpha`, turned into the reply's bytes in the same buffer, grown
/// for RGB, which is spread from the last pixel back so nothing is
/// overwritten before it is read. `None` when `data` holds fewer pixels or
/// the buffer cannot grow.
pub fn into_reply(
    layout: Layout,
    alpha: Alpha,
    mut data: Vec<u8>,
    pixels: usize,
) -> Option<Vec<u8>> {
    let len = pixels.checked_mul(4)?;
    if data.len() < pixels.checked_mul(layout.bytes())? {
        return None;
    }
    match layout {
        Layout::Rgba => {
            data.truncate(len);
            for group in data.as_chunks_mut::<4>().0 {
                let px = reply_bytes(pixel(layout, alpha, group.as_slice()));
                group.copy_from_slice(&px);
            }
        }
        Layout::Rgb => {
            data.truncate(pixels.checked_mul(3)?);
            data.try_reserve_exact(len.saturating_sub(data.len()))
                .ok()?;
            data.resize(len, 0);
            for i in (0..pixels).rev() {
                let from = i.checked_mul(3)?;
                let rgb = [
                    *data.get(from)?,
                    *data.get(from.checked_add(1)?)?,
                    *data.get(from.checked_add(2)?)?,
                ];
                let px = reply_bytes(pixel(layout, alpha, &rgb));
                let to = i.checked_mul(4)?;
                data.get_mut(to..to.checked_add(4)?)?.copy_from_slice(&px);
            }
        }
    }
    Some(data)
}

/// [`to_reply`] for straight R, G, B, A bytes in place: each pixel of
/// `rgba` is premultiplied where it lies, so a decoder's buffer becomes the
/// reply with no second buffer.
pub fn premultiply_in_place(rgba: &mut [u8]) {
    for [r, g, b, a] in rgba.as_chunks_mut::<4>().0 {
        let [pr, pg, pb, pa] = reply_bytes(premultiply(*r, *g, *b, *a));
        (*r, *g, *b, *a) = (pr, pg, pb, pa);
    }
}

/// The store's pixels of a reply's premultiplied R, G, B, A `bytes`, into
/// `out`, emptied first, and whether every pixel is opaque. Each color is
/// clamped to its pixel's alpha and each byte read once, so a reply's
/// pixels can never hold a color past their alpha. A partial pixel at the
/// end is left out.
pub fn from_reply(bytes: &[u8], mut out: Vec<u32>) -> (Vec<u32>, bool) {
    out.clear();
    out.reserve_exact(bytes.len().checked_div(4).unwrap_or(0));
    let mut alphas = 0xffu8;
    extend_from_reply(bytes, &mut out, &mut alphas);
    (out, alphas == 0xff)
}

/// [`from_reply`] for one run of a reply's bytes: their pixels, clamped,
/// appended to `out`, and their alphas ANDed into `alphas`. A reply read in
/// runs goes through here run by run, so it is copied and clamped once, as
/// a reply read whole is.
pub fn extend_from_reply(bytes: &[u8], out: &mut Vec<u32>, alphas: &mut u8) {
    out.extend(bytes.as_chunks::<4>().0.iter().map(|&[r, g, b, a]| {
        *alphas &= a;
        clamped(r, g, b, a)
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_alpha_becomes_premultiplied_with_integer_rounding() {
        assert_eq!(premultiply(0xff, 0x80, 0x00, 0xff), 0xffff_8000);
        assert_eq!(premultiply(0xff, 0xff, 0xff, 0x80), 0x8080_8080);
        // (0x80 × 0x80 + 127) / 255 = 64.75, down to 0x40
        assert_eq!(premultiply(0x80, 0x80, 0x80, 0x80), 0x8040_4040);
        // 3 x 0x80 / 255 is 1.5: 2 rounded, where truncation gives 1
        assert_eq!(premultiply(3, 3, 3, 0x80), 0x8002_0202);
        assert_eq!(premultiply(0xff, 0xff, 0xff, 0), 0);
    }

    #[test]
    fn each_layout_converts_into_the_reply_and_back_unchanged() {
        let mut out = Vec::new();
        to_reply(
            Layout::Rgb,
            Alpha::Straight,
            &[1, 2, 3, 4, 5, 6, 7],
            &mut out,
        );
        assert_eq!(out, [1, 2, 3, 0xff, 4, 5, 6, 0xff]);
        out.clear();
        to_reply(
            Layout::Rgba,
            Alpha::Straight,
            &[0xff, 0xff, 0xff, 0x80, 9, 9, 9, 0],
            &mut out,
        );
        assert_eq!(out, [0x80, 0x80, 0x80, 0x80, 0, 0, 0, 0]);
        out.clear();
        // premultiplied colors past their alpha are clamped
        to_reply(
            Layout::Rgba,
            Alpha::Premultiplied,
            &[0x90, 0x10, 0x80, 0x80],
            &mut out,
        );
        assert_eq!(out, [0x80, 0x10, 0x80, 0x80]);
        let (pixels, opaque) = from_reply(&out, Vec::new());
        assert_eq!(pixels, [0x8080_1080]);
        assert!(!opaque);
        let mut straight = [0xff, 0x80, 0, 0xff, 0xff, 0xff, 0xff, 0x80];
        premultiply_in_place(&mut straight);
        assert_eq!(straight, [0xff, 0x80, 0, 0xff, 0x80, 0x80, 0x80, 0x80]);
    }

    #[test]
    fn in_place_conversion_is_the_borrowed_one() {
        let rgb: Vec<u8> = (0..30u8).collect();
        let mut want = Vec::new();
        to_reply(Layout::Rgb, Alpha::Straight, &rgb, &mut want);
        assert_eq!(
            into_reply(Layout::Rgb, Alpha::Straight, rgb.clone(), 10),
            Some(want)
        );
        let rgba: Vec<u8> = (0..40u8).map(|b| b.wrapping_mul(37)).collect();
        for alpha in [Alpha::Straight, Alpha::Premultiplied] {
            let mut want = Vec::new();
            to_reply(Layout::Rgba, alpha, &rgba, &mut want);
            assert_eq!(
                into_reply(Layout::Rgba, alpha, rgba.clone(), 10),
                Some(want)
            );
        }
        // bytes past the pixels are dropped; too few is none
        assert_eq!(
            into_reply(Layout::Rgb, Alpha::Straight, rgb.clone(), 2).map(|v| v.len()),
            Some(8)
        );
        assert_eq!(into_reply(Layout::Rgb, Alpha::Straight, rgb, 11), None);
    }

    #[test]
    fn a_reply_with_colors_past_their_alpha_is_clamped_and_its_opacity_found() {
        let (pixels, opaque) = from_reply(&[0xff, 0xff, 0xff, 0x10, 1, 2, 3, 0xff], vec![7; 9]);
        assert_eq!(pixels, [0x1010_1010, 0xff01_0203]);
        assert!(!opaque);
        let (pixels, opaque) = from_reply(&[1, 2, 3, 0xff, 4, 5, 6, 0xff, 9], Vec::new());
        assert_eq!(pixels, [0xff01_0203, 0xff04_0506]);
        assert!(opaque);
        // a buffer handed in is emptied and filled, its memory kept
        let spare = vec![7u32; 4];
        let ptr = spare.as_ptr();
        let (out, _) = from_reply(&[0xff; 16], spare);
        assert_eq!((out.as_ptr(), out.len()), (ptr, 4));
        assert!(out.iter().all(|&px| px == 0xffff_ffff));
    }
}
