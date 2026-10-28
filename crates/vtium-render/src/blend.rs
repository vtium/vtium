//! The per-pixel arithmetic, carried forward from the C reference
//! (ADR-1300, "Carried forward"): tier 2's goldens were drawn with it, so
//! vtium draws with exactly the same integers.
//!
//! A display pixel is `0x00RRGGBB`. The composing pixel is premultiplied
//! `0xAARRGGBB`, its colors already multiplied by its alpha.
//!
//! Every input is a byte, so no sum or product here passes 255 * 255 +
//! 255; the wrapping operations never wrap.

use vtium_term::Rgb;

/// `t / 255` rounded, exact for `t` up to 255 * 255, without a division.
#[inline]
pub(crate) fn div255(t: u32) -> u32 {
    let t = t.wrapping_add(0x80);
    t.wrapping_add(t >> 8) >> 8
}

/// `c` as a display pixel.
#[inline]
pub(crate) fn pixel(c: Rgb) -> u32 {
    u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b)
}

/// One channel of `f` over `b` through coverage `a`, `na` being `255 - a`.
#[inline]
fn lerp(f: u8, b: u8, a: u32, na: u32) -> u32 {
    div255(
        u32::from(f)
            .wrapping_mul(a)
            .wrapping_add(u32::from(b).wrapping_mul(na)),
    )
}

/// `fg` painted over `bg` through coverage `a`: a cell drawn from its
/// background and one mask.
#[inline]
pub(crate) fn mask(fg: Rgb, bg: Rgb, a: u8) -> u32 {
    match a {
        0 => pixel(bg),
        255 => pixel(fg),
        _ => {
            let a = u32::from(a);
            let na = 255u32.wrapping_sub(a);
            lerp(fg.r, bg.r, a, na) << 16 | lerp(fg.g, bg.g, a, na) << 8 | lerp(fg.b, bg.b, a, na)
        }
    }
}

/// The premultiplied `src` over the opaque `bg`: `src + bg * (255 - a)`.
#[inline]
pub(crate) fn over_bg(bg: Rgb, src: u32) -> u32 {
    let a = src >> 24;
    match a {
        0 => pixel(bg),
        255 => src & 0x00ff_ffff,
        _ => {
            let na = 255u32.wrapping_sub(a);
            let ch = |s: u32, b: u8| (s & 0xff).wrapping_add(div255(u32::from(b).wrapping_mul(na)));
            ch(src >> 16, bg.r) << 16 | ch(src >> 8, bg.g) << 8 | ch(src, bg.b)
        }
    }
}

/// The premultiplied `a`, `r`, `g`, `b` over the premultiplied pixel `p`.
#[inline]
fn over(p: u32, a: u32, r: u32, g: u32, b: u32) -> u32 {
    let na = 255u32.wrapping_sub(a);
    let ch = |s: u32, shift: u32| s.wrapping_add(div255((p >> shift & 0xff).wrapping_mul(na)));
    ch(a, 24) << 24 | ch(r, 16) << 16 | ch(g, 8) << 8 | ch(b, 0)
}

/// Coverage `a` tinted with `c`, over the premultiplied pixel `p`: a mask
/// or a decoration composed onto a cell of several layers.
#[inline]
pub(crate) fn tint_over(p: u32, a: u8, c: Rgb) -> u32 {
    if a == 0 {
        return p;
    }
    let a = u32::from(a);
    let t = |v: u8| div255(u32::from(v).wrapping_mul(a));
    over(p, a, t(c.r), t(c.g), t(c.b))
}

/// The premultiplied color pixel `src` over the premultiplied pixel `p`.
#[inline]
pub(crate) fn argb_over(p: u32, src: u32) -> u32 {
    let a = src >> 24;
    if a == 0 {
        return p;
    }
    over(p, a, src >> 16 & 0xff, src >> 8 & 0xff, src & 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;

    // drm2d_render.c:46-51: exact for every product of two bytes
    #[test]
    fn div255_rounds_to_nearest_over_its_range() {
        for t in 0..=255 * 255 {
            let want = (t * 2 + 255) / 510;
            assert_eq!(div255(t), want, "{t}");
        }
    }

    #[test]
    fn a_mask_lerps_from_background_to_foreground() {
        let fg = Rgb::new(200, 100, 0);
        let bg = Rgb::new(0, 50, 255);
        assert_eq!(mask(fg, bg, 0), 0x0032ff);
        assert_eq!(mask(fg, bg, 255), 0xc86400);
        // div255(200 * 128) = 100, div255(100 * 128 + 50 * 127) = 75,
        // div255(255 * 127) = 127
        assert_eq!(mask(fg, bg, 128), 100 << 16 | 75 << 8 | 127);
    }

    #[test]
    fn premultiplied_layers_go_over_their_background() {
        let bg = Rgb::new(10, 20, 30);
        assert_eq!(over_bg(bg, 0), 0x0a141e);
        assert_eq!(over_bg(bg, 0xff01_0203), 0x010203);
        // half of white over the background
        let half = tint_over(0, 128, Rgb::new(255, 255, 255));
        assert_eq!(half, 0x8080_8080);
        assert_eq!(
            over_bg(bg, half),
            (128 + 5) << 16 | (128 + 10) << 8 | (128 + 15)
        );
        assert_eq!(argb_over(0, half), half);
        assert_eq!(argb_over(half, 0), half);
    }
}
