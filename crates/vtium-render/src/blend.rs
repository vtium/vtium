//! The per-pixel arithmetic (ADR-1300, "Carried forward"), in whole
//! numbers, so a frame draws to the same bytes on every machine and tier
//! 2's goldens judge it exactly.
//!
//! A display pixel is `0x00RRGGBB`. The composing pixel is premultiplied
//! `0xAARRGGBB`, its colors already multiplied by its alpha.
//!
//! Every input is a byte, so no sum or product here passes 255 * 255 +
//! 255; the wrapping operations never wrap.

use vtium_term::Rgb;

#[cfg(any(feature = "images", feature = "scroll-pan"))]
pub(crate) mod linear;

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

/// The premultiplied picture pixel `src` over the opaque display pixel
/// `dst`, mixed in linear light: what a partly transparent picture pixel
/// shows over the background, the text or another picture below it.
///
/// An opaque pixel replaces `dst`, and a clear one leaves it as it is. For
/// any other alpha, each channel is turned back into its straight color,
/// both colors are turned into linear light, they are mixed by the alpha
/// in 16-bit fixed point, and the mix is turned back into sRGB. Every step
/// is a whole number from a table, a multiply, a shift or a division by a
/// constant, and no step passes 2^32. A channel above its alpha, which only
/// a malformed premultiplied pixel has, mixes as full brightness.
///
/// References:
/// - ADR-1300 D7: partly transparent picture pixels blend in linear light,
///   through the reciprocal, linear and sRGB tables.
#[cfg(feature = "images")]
#[inline]
pub(crate) fn picture_over(dst: u32, src: u32) -> u32 {
    let [a, r, g, b] = src.to_be_bytes();
    match a {
        0 => dst,
        255 => src & 0x00ff_ffff,
        _ => {
            let [_, dr, dg, db] = dst.to_be_bytes();
            let recip = linear::RECIPROCAL.get(usize::from(a)).copied().unwrap_or(0);
            let a = u32::from(a);
            let na = 255u32.wrapping_sub(a);
            let ch = |s: u8, d: u8| linear_channel(s, d, recip, a, na);
            ch(r, dr) << 16 | ch(g, dg) << 8 | ch(b, db)
        }
    }
}

/// One channel of [`picture_over`] for a partial alpha `a`: the
/// premultiplied `s` over `d`, with `recip` the alpha's entry in
/// [`linear::RECIPROCAL`] and `na` `255 - a`.
///
/// Full brightness is 65,520 in linear light, 16 times the sRGB table's
/// 4,095 steps. So one rounded division by `255 * 16` both mixes by the
/// alpha and finds the mix's step in [`linear::TO_SRGB`].
///
/// The straight color is at most `255 * RECIPROCAL[1] + 32,768`, under
/// 2^32, before its shift, and the mix at most `65,520 * 255 + 2,040`. So
/// no product wraps, and the step is at most 4,095.
#[cfg(feature = "images")]
#[inline]
fn linear_channel(s: u8, d: u8, recip: u32, a: u32, na: u32) -> u32 {
    let straight = u32::from(s).wrapping_mul(recip).wrapping_add(0x8000) >> 16;
    let straight = usize::try_from(straight.min(255)).unwrap_or(255);
    let lc = u32::from(linear::TO_LINEAR.get(straight).copied().unwrap_or(0));
    let ld = u32::from(linear::TO_LINEAR.get(usize::from(d)).copied().unwrap_or(0));
    let step = lc
        .wrapping_mul(a)
        .wrapping_add(ld.wrapping_mul(na))
        .wrapping_add(2040)
        .wrapping_div(4080);
    let step = usize::try_from(step.min(4095)).unwrap_or(4095);
    u32::from(linear::TO_SRGB.get(step).copied().unwrap_or(255))
}

/// The display pixels `a` and `b` mixed in linear light, `a` weighted by
/// `keep` of 255 and `b` by the rest: what the fade of the spare lines
/// shows, by the same tables a partly transparent picture blends through.
///
/// References:
/// - ADR-1310 D12, "The key `partial-row`": the fade mixes in linear light, as a picture blends.
/// - ADR-1300 D7: the reciprocal, linear and sRGB tables.
#[cfg(feature = "scroll-pan")]
pub(crate) fn mix_linear(a: u32, b: u32, keep: u8) -> u32 {
    let k = u32::from(keep);
    let nk = 255u32.wrapping_sub(k);
    let ch = |shift: u32| {
        let lin = |p: u32| {
            let byte = usize::try_from(p >> shift & 0xff).unwrap_or(0);
            u32::from(linear::TO_LINEAR.get(byte).copied().unwrap_or(0))
        };
        let step = lin(a)
            .wrapping_mul(k)
            .wrapping_add(lin(b).wrapping_mul(nk))
            .wrapping_add(2040)
            .wrapping_div(4080);
        let step = usize::try_from(step.min(4095)).unwrap_or(4095);
        u32::from(linear::TO_SRGB.get(step).copied().unwrap_or(255))
    };
    ch(16) << 16 | ch(8) << 8 | ch(0)
}

#[cfg(test)]
#[cfg_attr(
    feature = "images",
    expect(clippy::indexing_slicing, reason = "a test fails by panicking")
)]
mod tests {
    use super::*;

    // exact for every product of two bytes
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

    /// A partly transparent picture pixel's channel, `s` premultiplied at
    /// alpha `a`, over the display channel `d`, as `picture_over` blends
    /// it.
    #[cfg(feature = "images")]
    fn picture_channel(s: u32, a: u32, d: u32) -> u32 {
        picture_over(d << 16, a << 24 | s << 16) >> 16
    }

    // ADR-1300 D7's example: red 204 at alpha 17 over black. The store
    // holds red premultiplied, div255(204 * 17) = 14. Straight again:
    // RECIPROCAL[17] = (255 * 65536 + 8) / 17 = 983,040, and
    // (14 * 983,040 + 32,768) >> 16 = 210. In linear light,
    // TO_LINEAR[210] = 42,226 and TO_LINEAR[0] = 0. Mixed, on the sRGB
    // table's steps: (42,226 * 17 + 0 * 238 + 2,040) / 4,080 = 176, and
    // TO_SRGB[176] = 58. Mixed in sRGB, as cells mix, it would be 14.
    #[cfg(feature = "images")]
    #[test]
    fn red_204_at_alpha_17_over_black_blends_in_linear_light_to_58() {
        assert_eq!(linear::RECIPROCAL[17], 983_040);
        assert_eq!(linear::TO_LINEAR[210], 42_226);
        assert_eq!(linear::TO_SRGB[176], 58);
        assert_eq!(picture_channel(div255(204 * 17), 17, 0), 58);
        assert_eq!(picture_over(0, 17 << 24 | 14 << 16), 58 << 16);
    }

    // Each channel by hand from the tables, mixed as
    // (TO_LINEAR[straight] * a + TO_LINEAR[d] * (255 - a) + 2,040) / 4,080:
    // - white at alpha 128 over 64: div255(255 * 128) = 128, straight
    //   (128 * 130,560 + 32,768) >> 16 = 255; TO_LINEAR[255] = 65,520 and
    //   TO_LINEAR[64] = 3,359; (65,520 * 128 + 3,359 * 127 + 2,040) /
    //   4,080 = 2,160, and TO_SRGB[2160] = 192. In sRGB it would be 160.
    // - at alpha 64, RECIPROCAL[64] = 261,120:
    //   - red 255 over 0: 64 stored, straight 255; (65,520 * 64 + 2,040) /
    //     4,080 = 1,028, and TO_SRGB[1028] = 137. In sRGB, 64.
    //   - green 0 over 255: straight 0; (65,520 * 191 + 2,040) / 4,080 =
    //     3,067, and TO_SRGB[3067] = 224. In sRGB, 191.
    //   - blue 100 over 30: div255(100 * 64) = 25, straight
    //     (25 * 261,120 + 32,768) >> 16 = 100; TO_LINEAR[100] = 8,350 and
    //     TO_LINEAR[30] = 851; (8,350 * 64 + 851 * 191 + 2,040) / 4,080 =
    //     171, and TO_SRGB[171] = 58. In sRGB, 47.
    #[cfg(feature = "images")]
    #[test]
    fn each_channel_blends_through_the_tables_on_its_own() {
        assert_eq!(linear::TO_LINEAR[64], 3_359);
        assert_eq!(linear::TO_LINEAR[100], 8_350);
        assert_eq!(linear::TO_LINEAR[30], 851);
        assert_eq!(picture_channel(128, 128, 64), 192);
        let src = 64 << 24 | div255(255 * 64) << 16 | div255(100 * 64);
        assert_eq!(picture_over(0x00_ff1e, src), 137 << 16 | 224 << 8 | 58);
    }

    #[cfg(feature = "images")]
    #[test]
    fn an_opaque_picture_pixel_replaces_and_a_clear_one_keeps_the_pixel_below() {
        assert_eq!(picture_over(0x12_3456, 0xff65_4321), 0x65_4321);
        assert_eq!(picture_over(0x12_3456, 0x0000_0000), 0x12_3456);
        // a malformed clear pixel, its colors past its alpha of 0
        assert_eq!(picture_over(0x12_3456, 0x00ff_ffff), 0x12_3456);
    }

    // A channel above its alpha mixes as full brightness: 200 at alpha 100
    // is straight 255, as white at alpha 100 is.
    #[cfg(feature = "images")]
    #[test]
    fn a_channel_past_its_alpha_blends_as_full_brightness() {
        assert_eq!(picture_channel(200, 100, 40), picture_channel(100, 100, 40));
        assert_eq!(picture_channel(255, 1, 0), picture_channel(1, 1, 0));
    }

    /// sRGB's transfer function from a byte to linear light, 0 to 1.
    #[cfg(feature = "images")]
    fn decode(v: u32) -> f64 {
        let x = f64::from(v) / 255.0;
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    }

    /// The inverse of [`decode`], from linear light to a byte, unrounded.
    #[cfg(feature = "images")]
    fn encode(y: f64) -> f64 {
        let s = if y <= 0.003_130_8 {
            12.92 * y
        } else {
            1.055 * y.powf(1.0 / 2.4) - 0.055
        };
        s * 255.0
    }

    // Each entry is its definition's value, rounded. The check allows a
    // value a millionth past a half, so that a last-bit difference in a
    // machine's powf cannot fail it.
    #[cfg(feature = "images")]
    #[test]
    fn the_tables_hold_their_definitions_rounded() {
        let near = |got: u32, want: f64| (f64::from(got) - want).abs() <= 0.500_001;
        for (v, &l) in (0u32..).zip(linear::TO_LINEAR.iter()) {
            assert!(
                near(u32::from(l), 65520.0 * decode(v)),
                "TO_LINEAR[{v}] = {l}"
            );
        }
        for (i, &s) in (0u32..).zip(linear::TO_SRGB.iter()) {
            let want = encode(f64::from(i) / 4095.0);
            assert!(near(u32::from(s), want), "TO_SRGB[{i}] = {s}, want {want}");
        }
        assert_eq!(linear::RECIPROCAL[0], 0);
        for (a, &r) in (0u32..).zip(linear::RECIPROCAL.iter()).skip(1) {
            let want = 255.0 * 65536.0 / f64::from(a);
            assert!(near(r, want), "RECIPROCAL[{a}] = {r}, want {want}");
        }
        // each byte taken into linear light and back is the byte again,
        // so a color mixed with itself keeps its color
        for (v, &l) in (0u8..=255).zip(linear::TO_LINEAR.iter()) {
            let step = (u32::from(l) + 8) / 16;
            assert_eq!(linear::TO_SRGB[step as usize], v, "{v}");
        }
    }

    // ADR-1300 D7's sweep: every color at every partial alpha, stored
    // premultiplied as the store keeps it, over four backgrounds, against
    // an exact blend in linear light in floating point, rounded. The store's
    // rounding of a premultiplied byte costs at most 10 levels below alpha
    // 16, 3 at 16 and above, and 2 at 64 and above.
    #[cfg(feature = "images")]
    #[test]
    fn every_color_and_alpha_blends_within_the_store_s_precision() {
        let mut worst = [0u32; 3];
        for d in [0u32, 64, 128, 255] {
            for a in 1..=254u32 {
                let band = usize::from(a >= 16) + usize::from(a >= 64);
                for c in 0..=255u32 {
                    let got = picture_channel(div255(c * a), a, d);
                    let w = f64::from(a) / 255.0;
                    let exact = encode(decode(c) * w + decode(d) * (1.0 - w)).round();
                    let off = (f64::from(got) - exact).abs();
                    assert!(off <= 10.0, "{c} at {a} over {d}: {got}, exact {exact}");
                    // off is a whole number of levels at most 10
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a whole number from 0 to 10"
                    )]
                    let off = off as u32;
                    worst[band] = worst[band].max(off);
                }
            }
        }
        assert_eq!(
            worst,
            [10, 3, 2],
            "the worst error below 16, from 16, from 64"
        );
    }
}
