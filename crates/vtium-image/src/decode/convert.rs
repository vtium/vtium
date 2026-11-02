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

/// [`to_reply`] for straight R, G, B, A bytes in place: each pixel of
/// `rgba` is premultiplied where it lies, so a decoder's buffer becomes the
/// reply with no second buffer.
pub fn premultiply_in_place(rgba: &mut [u8]) {
    for group in rgba.chunks_exact_mut(4) {
        if let [r, g, b, a] = group {
            let [pr, pg, pb, pa] = reply_bytes(premultiply(*r, *g, *b, *a));
            (*r, *g, *b, *a) = (pr, pg, pb, pa);
        }
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
    out.extend(bytes.chunks_exact(4).map(|group| match *group {
        [r, g, b, a] => {
            alphas &= a;
            clamped(r, g, b, a)
        }
        _ => 0,
    }));
    (out, alphas == 0xff)
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
