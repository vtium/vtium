//! `sin` and `cos` from IEEE 754 addition, subtraction and multiplication
//! only, so the curly underline is the same bytes on every host: a
//! platform's libm may round them differently. The algorithm is FreeBSD's
//! msun (`k_sin.c`, `k_cos.c` and `e_rem_pio2.c`), by way of musl, whose
//! notice is:
//!
//! Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//!
//! Developed at SunSoft, a Sun Microsystems, Inc. business.
//! Permission to use, copy, modify, and distribute this
//! software is freely granted, provided that this notice
//! is preserved.
//!
//! Only the argument reduction for `|x|` below 2^20 * pi/2 is kept, which
//! covers every angle a sprite asks for.

const TOINT: f64 = 1.5 / f64::EPSILON;
const PIO4: f64 = f64::from_bits(0x3fe9_21fb_5444_2d18);
const INVPIO2: f64 = f64::from_bits(0x3fe4_5f30_6dc9_c883);
const PIO2_1: f64 = f64::from_bits(0x3ff9_21fb_5440_0000);
const PIO2_1T: f64 = f64::from_bits(0x3dd0_b461_1a62_6331);
const PIO2_2: f64 = f64::from_bits(0x3dd0_b461_1a60_0000);
const PIO2_2T: f64 = f64::from_bits(0x3ba3_198a_2e03_7073);
const PIO2_3: f64 = f64::from_bits(0x3ba3_198a_2e00_0000);
const PIO2_3T: f64 = f64::from_bits(0x397b_839a_2520_49c1);

const S1: f64 = f64::from_bits(0xbfc5_5555_5555_5549);
const S2: f64 = f64::from_bits(0x3f81_1111_1110_f8a6);
const S3: f64 = f64::from_bits(0xbf2a_01a0_19c1_61d5);
const S4: f64 = f64::from_bits(0x3ec7_1de3_57b1_fe7d);
const S5: f64 = f64::from_bits(0xbe5a_e5e6_8a2b_9ceb);
const S6: f64 = f64::from_bits(0x3de5_d93a_5acf_d57c);

const C1: f64 = f64::from_bits(0x3fa5_5555_5555_554c);
const C2: f64 = f64::from_bits(0xbf56_c16c_16c1_5177);
const C3: f64 = f64::from_bits(0x3efa_01a0_19cb_1590);
const C4: f64 = f64::from_bits(0xbe92_7e4f_809c_52ad);
const C5: f64 = f64::from_bits(0x3e21_ee9e_bdb4_b1c4);
const C6: f64 = f64::from_bits(0xbda8_fae9_be88_38d4);

/// sin(x + y) for |x + y| <= pi/4, `y` the tail of `x`.
fn k_sin(x: f64, y: f64) -> f64 {
    let z = x * x;
    let w = z * z;
    let r = S2 + z * (S3 + z * S4) + z * w * (S5 + z * S6);
    let v = z * x;
    x - ((z * (0.5 * y - v * r) - y) - v * S1)
}

/// cos(x + y) for |x + y| <= pi/4.
fn k_cos(x: f64, y: f64) -> f64 {
    let z = x * x;
    let w = z * z;
    let r = z * (C1 + z * (C2 + z * C3)) + w * w * (C4 + z * (C5 + z * C6));
    let hz = 0.5 * z;
    let w = 1.0 - hz;
    w + (((1.0 - w) - hz) + (z * r - x * y))
}

/// The biased exponent of `x`.
fn exponent(x: f64) -> i32 {
    i32::try_from(x.to_bits() >> 52 & 0x7ff).unwrap_or(0)
}

/// `x` less the nearest multiple `n` of pi/2, as a head and a tail, with
/// `n mod 4`: msun's medium case, for |x| below 2^20 * pi/2.
fn rem_pio2(x: f64) -> (u8, f64, f64) {
    let mut f = x * INVPIO2 + TOINT - TOINT;
    let mut r = x - f * PIO2_1;
    let mut w = f * PIO2_1T;
    if r - w < -PIO4 {
        f -= 1.0;
        r = x - f * PIO2_1;
        w = f * PIO2_1T;
    } else if r - w > PIO4 {
        f += 1.0;
        r = x - f * PIO2_1;
        w = f * PIO2_1T;
    }
    let mut y0 = r - w;
    let ex = exponent(x);
    if ex.saturating_sub(exponent(y0)) > 16 {
        let t = r;
        w = f * PIO2_2;
        r = t - w;
        w = f * PIO2_2T - ((t - r) - w);
        y0 = r - w;
        if ex.saturating_sub(exponent(y0)) > 49 {
            let t = r;
            w = f * PIO2_3;
            r = t - w;
            w = f * PIO2_3T - ((t - r) - w);
            y0 = r - w;
        }
    }
    let y1 = (r - y0) - w;
    // f is a whole number below 2^20, so n is exact
    #[expect(
        clippy::cast_possible_truncation,
        reason = "f is a whole number below 2^20"
    )]
    let n = f as i32 & 3;
    let n = u8::try_from(n).unwrap_or(0);
    (n, y0, y1)
}

/// sin(x) for |x| below 2^20 * pi/2.
pub(crate) fn sin(x: f64) -> f64 {
    if x.abs() <= PIO4 {
        if x.abs() < f64::from_bits(0x3e50_0000_0000_0000) {
            return x;
        }
        return k_sin(x, 0.0);
    }
    let (n, y0, y1) = rem_pio2(x);
    match n {
        0 => k_sin(y0, y1),
        1 => k_cos(y0, y1),
        2 => -k_sin(y0, y1),
        _ => -k_cos(y0, y1),
    }
}

/// cos(x) for |x| below 2^20 * pi/2.
pub(crate) fn cos(x: f64) -> f64 {
    if x.abs() <= PIO4 {
        if x.abs() < f64::from_bits(0x3e46_a09e_0000_0000) {
            return 1.0;
        }
        return k_cos(x, 0.0);
    }
    let (n, y0, y1) = rem_pio2(x);
    match n {
        0 => k_cos(y0, y1),
        1 => -k_sin(y0, y1),
        2 => -k_cos(y0, y1),
        _ => k_sin(y0, y1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Within an ulp of the platform's, over the angles a sprite asks for;
    // the sprites themselves are checked against the C reference's.
    #[test]
    fn agrees_with_the_platform_to_an_ulp() {
        let ulps = |a: f64, b: f64| a.to_bits().abs_diff(b.to_bits());
        let mut worst = 0;
        for i in 0..200_000 {
            let x = f64::from(i) * 7.0 / 200_000.0;
            for (a, b) in [(sin(x), x.sin()), (cos(x), x.cos())] {
                if a.abs() > 1e-300 {
                    worst = worst.max(ulps(a, b));
                }
            }
        }
        assert!(worst <= 1, "{worst} ulps");
        assert_eq!(sin(0.0), 0.0);
        assert_eq!(cos(0.0), 1.0);
    }
}
