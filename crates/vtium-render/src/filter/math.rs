//! The arithmetic the color table is built from: a power, an exponential and
//! a logarithm written with IEEE 754's correctly rounded operations alone,
//! and the conversions between a 64-bit float and a half-precision float.
//!
//! A platform's maths library may round the last bits of `powf`, `exp` and
//! `ln` its own way, and a fused multiply-add rounds once where a multiply
//! and an add round twice. Either would make the table's bytes differ from
//! one machine to the next. Rust never fuses a multiply and an add on its
//! own, and every function here uses only addition, subtraction,
//! multiplication, division and exact bit manipulation, so two machines
//! build the same table to the bit.
//!
//! References:
//! - ADR-1304 D2: the table is built from correctly rounded operations, never the platform's maths library.

use std::f64::consts::{LN_2, SQRT_2};

/// The natural logarithm of `x`, for `x` above 0 and finite. At 0 and
/// below it is negative infinity, which [`pow`] never asks for.
///
/// `x` is split into a power of two and a mantissa `m` between √½ and √2,
/// and ln m is the series 2(s + s³/3 + s⁵/5 + …) with s = (m − 1)/(m + 1).
/// |s| stays under 0.172, so 14 terms bring the error below 10⁻¹⁸.
pub(crate) fn ln(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return x;
    }
    // a subnormal is scaled into the normal range first, exactly
    let (x, shift) = if x < f64::MIN_POSITIVE {
        (x * 18_014_398_509_481_984.0, -54)
    } else {
        (x, 0)
    };
    let bits = x.to_bits();
    let biased = i32::try_from(bits >> 52 & 0x7ff).unwrap_or(1023);
    let mut e = biased.saturating_sub(1023).saturating_add(shift);
    let mut m = f64::from_bits(bits & 0x000f_ffff_ffff_ffff | 0x3ff0_0000_0000_0000);
    if m > SQRT_2 {
        m /= 2.0;
        e = e.saturating_add(1);
    }
    let s = (m - 1.0) / (m + 1.0);
    let s2 = s * s;
    let mut sum = 0.0;
    let mut term = s;
    let mut odd = 1.0;
    for _ in 0..14 {
        sum += term / odd;
        term *= s2;
        odd += 2.0;
    }
    2.0 * sum + f64::from(e) * LN_2
}

/// e raised to `y`, for `y` from −700 to 700; 0 below and infinity above.
///
/// `y` is split into k ln 2 + r, with k whole and |r| at most ½ ln 2, so
/// e^y = 2^k e^r. e^r is its Taylor series to 20 terms, whose error stays
/// below 10⁻²⁷, and 2^k is built from its bits exactly.
pub(crate) fn exp(y: f64) -> f64 {
    if y.is_nan() {
        return 0.0;
    }
    if y < -700.0 {
        return 0.0;
    }
    if y > 700.0 {
        return f64::INFINITY;
    }
    let k = (y / LN_2).round();
    let r = y - k * LN_2;
    let mut t = 1.0;
    let mut n = 20.0;
    while n >= 1.0 {
        t = 1.0 + r * t / n;
        n -= 1.0;
    }
    // k lies within ±1010 here, inside an i32, and 2^k is a normal number
    // or, below 2^-1022, the product of two
    #[expect(
        clippy::cast_possible_truncation,
        reason = "k is a whole number within ±1010"
    )]
    let k = k as i32;
    if k < -1000 {
        return t * two_to(k.saturating_add(100)) * two_to(-100);
    }
    t * two_to(k)
}

/// 2 raised to `k`, for `k` from −1022 to 1023, built from its bits.
fn two_to(k: i32) -> f64 {
    let biased = u64::try_from(k.saturating_add(1023).clamp(1, 2046)).unwrap_or(1023);
    f64::from_bits(biased << 52)
}

/// `x` raised to `y`, for `x` at least 0 and `y` above 0: 0 at `x` = 0,
/// and e^(y ln x) above it.
pub(crate) fn pow(x: f64, y: f64) -> f64 {
    if x.is_nan() || x <= 0.0 {
        return 0.0;
    }
    if x == 1.0 || y == 1.0 {
        return x;
    }
    exp(y * ln(x))
}

/// `x` held to 0 to 1, NaN to 0.
pub(crate) fn unit(x: f64) -> f64 {
    if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
}

/// `x`, held to 0 to 1, as the half-precision float nearest it, ties to
/// even: IEEE 754's binary16, as an RGBA16F texture holds it.
pub(crate) fn half(x: f64) -> u16 {
    let x = unit(x);
    // the smallest normal half, 2^-14
    if x >= 6.103_515_625e-5 {
        let bits = x.to_bits();
        let biased = i32::try_from(bits >> 52 & 0x7ff).unwrap_or(1023);
        let mantissa = bits & 0x000f_ffff_ffff_ffff;
        let mut kept = mantissa >> 42;
        let rest = mantissa & 0x3ff_ffff_ffff;
        let tie = 0x200_0000_0000;
        if rest > tie || (rest == tie && kept & 1 == 1) {
            kept = kept.saturating_add(1);
        }
        // a mantissa that rounds up to 1024 carries into the exponent,
        // which the sum below does by itself
        let e = u64::try_from(biased.saturating_sub(1023).saturating_add(15)).unwrap_or(0);
        return u16::try_from((e << 10).saturating_add(kept)).unwrap_or(0x3c00);
    }
    // a subnormal half is n × 2^-24, and x × 2^24 is exact
    let v = x * 16_777_216.0;
    let n = v.floor();
    let rest = v - n;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "n is a whole number from 0 to 1024"
    )]
    let mut n = n as u16;
    if rest > 0.5 || (rest == 0.5 && n & 1 == 1) {
        n = n.saturating_add(1);
    }
    n
}

/// The half-precision float `h` as a 32-bit float, exactly.
pub(crate) fn from_half(h: u16) -> f32 {
    let e = u32::from(h >> 10 & 0x1f);
    let m = u32::from(h & 0x3ff);
    let sign = if h & 0x8000 == 0 { 1.0 } else { -1.0 };
    if e == 0 {
        // a subnormal: m × 2^-24, exact in a 32-bit float
        return sign * f32::from(u16::try_from(m).unwrap_or(0)) / 16_777_216.0;
    }
    if e == 31 {
        return if m == 0 {
            sign * f32::INFINITY
        } else {
            f32::NAN
        };
    }
    let bits = u32::from(h & 0x8000) << 16 | e.saturating_add(112) << 23 | m << 13;
    f32::from_bits(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Within a few units in the last place of the platform's own functions
    // over the colors' range, which shows the series are right; the table's
    // bytes are pinned in the tests of `table`.
    #[test]
    fn ln_exp_and_pow_agree_with_the_platform() {
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-14 * b.abs().max(1.0);
        for i in 1..=10_000 {
            let x = f64::from(i) / 10_000.0;
            assert!(close(ln(x), x.ln()), "ln {x}: {} {}", ln(x), x.ln());
            for y in [0.5, 1.0 / 1.2, 1.0 / 2.4, 2.4, 2.0] {
                assert!(close(pow(x, y), x.powf(y)), "{x}^{y}");
            }
        }
        for i in -7000..=7000 {
            let y = f64::from(i) / 100.0;
            assert!(close(exp(y), y.exp()), "exp {y}");
        }
        assert_eq!(pow(0.0, 2.0), 0.0);
        assert_eq!(pow(1.0, 0.3), 1.0);
        assert_eq!(exp(0.0), 1.0);
        assert_eq!(ln(1.0), 0.0);
        assert_eq!(ln(0.0), f64::NEG_INFINITY);
        assert!(ln(1e-310).is_finite());
        assert_eq!(exp(-800.0), 0.0);
    }

    // A half holds each node of the identity table, i/32, exactly, and
    // rounds to the nearest half, ties to even, as IEEE 754 asks.
    #[test]
    fn a_half_is_the_nearest_binary16() {
        assert_eq!(half(0.0), 0);
        assert_eq!(half(1.0), 0x3c00);
        assert_eq!(half(0.5), 0x3800);
        assert_eq!(half(2.0), 0x3c00, "held to 1");
        assert_eq!(half(-1.0), 0, "held to 0");
        assert_eq!(half(f64::NAN), 0);
        for i in 0..=32 {
            let x = f64::from(i) / 32.0;
            assert_eq!(f64::from(from_half(half(x))), x, "{i}/32");
        }
        // 1 - 2^-12 lies halfway between 0x3bff and 1, and ties to 1,
        // whose mantissa is even
        assert_eq!(half(1.0 - 1.0 / 4096.0), 0x3c00);
        let below_one = 1.0 - 1.0 / 2048.0;
        assert_eq!(half(below_one), 0x3bff);
        // just above the tie between 0x3bfe and 0x3bff rounds up
        let tie = (f64::from(from_half(0x3bfe)) + f64::from(from_half(0x3bff))) / 2.0;
        assert_eq!(half(tie), 0x3bfe, "a tie goes to the even mantissa");
        assert_eq!(half(tie + 1e-12), 0x3bff);
        // the smallest subnormal, and the largest subnormal carrying into
        // the smallest normal
        assert_eq!(half(1.0 / 16_777_216.0), 1);
        assert_eq!(half(1023.6 / 16_777_216.0), 0x0400);
        assert_eq!(from_half(1), 1.0 / 16_777_216.0);
        assert_eq!(from_half(0x0400), 6.103_515_6e-5);
        // every half from 0 to 1 survives the round trip
        for h in 0..=0x3c00u16 {
            assert_eq!(half(f64::from(from_half(h))), h, "{h:#06x}");
        }
    }
}
