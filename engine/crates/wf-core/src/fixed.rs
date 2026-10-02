//! Integer helpers that reproduce the reference semantics exactly (the TS port's `>>`, `|0`, `Math.trunc`,
//! `Math.floor(v / 2**n)`, `Math.round` of rationals). All simulation math is integer; nothing here uses floats.

/// arithmetic shift right (ARM `asrs`, JS `>>` on int32 values)
#[inline]
pub fn asr(v: i64, n: u32) -> i64 {
    v >> n
}

/// truncating division, 0 when b == 0 (the reference `tdiv`)
#[inline]
pub fn tdiv(a: i64, b: i64) -> i64 {
    if b == 0 {
        0
    } else {
        a / b
    }
}

/// `Math.floor(a / b)` for b > 0
#[inline]
pub fn fdiv(a: i64, b: i64) -> i64 {
    a.div_euclid(b)
}

/// `Math.round(a / b)` (round half toward +inf), b != 0
#[inline]
pub fn round_div(a: i64, b: i64) -> i64 {
    let (a, b) = if b < 0 { (-a, -b) } else { (a, b) };
    (2 * a + b).div_euclid(2 * b)
}

/// JS `Math.sign` on integers
#[inline]
pub fn sign(v: i64) -> i64 {
    v.signum()
}

/// floor(sqrt(n)) for n >= 0
pub fn isqrt(n: i64) -> i64 {
    isqrt128(n.max(0) as i128) as i64
}

/// floor(sqrt(n)) (integer Newton iteration)
pub fn isqrt128(n: i128) -> i128 {
    if n < 2 {
        return n.max(0);
    }
    let mut x = n;
    let mut y = (x + 1) >> 1;
    while y < x {
        x = y;
        y = (x + n / x) >> 1;
    }
    x
}

/// `Math.round(num / Math.hypot(dx, dy))` computed exactly: the integer r with
/// (r - 1/2) <= num / sqrt(s) < (r + 1/2), s = dx^2 + dy^2 > 0 (ties toward +inf as Math.round).
/// Equal to the double-precision reference except in non-representable tie cases (which require sqrt(s) rational,
/// where the double computation is exact as well).
pub fn round_div_sqrt(num: i64, s: i64) -> i64 {
    debug_assert!(s > 0);
    let neg = num < 0;
    let n = num.unsigned_abs() as i128;
    let s = s as i128;
    // q = |num| / sqrt(s); want floor(q + 1/2) for positive, and for negative: round(-q) = -floor(q - 1/2) when not a tie
    // compute f = floor(q) exactly: largest f with f^2 * s <= n^2
    let n2 = n * n;
    // floor(sqrt(n^2 / s)) == floor(sqrt(floor(n^2 / s)))
    let f = isqrt128(n2 / s);
    // fractional part >= 1/2  <=>  (2f + 1)^2 * s <= 4 n^2
    let half_or_more = (2 * f + 1) * (2 * f + 1) * s <= 4 * n2;
    let exact_half = (2 * f + 1) * (2 * f + 1) * s == 4 * n2;
    let r = if !neg {
        if half_or_more {
            f + 1
        } else {
            f
        }
    } else {
        // Math.round(-q): ties go toward +inf, i.e. -(f) when exactly f + 1/2
        if exact_half {
            -f
        } else if half_or_more {
            -(f + 1)
        } else {
            -f
        }
    };
    r as i64
}

/// `Math.round(sin(k * PI / 16) * 8)` for k = 0..15 (hop arcs)
pub const HOP_ARC8: [i32; 16] = [0, 2, 3, 4, 6, 7, 7, 8, 8, 8, 7, 7, 6, 4, 3, 2];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_div_sqrt_matches_float() {
        for dx in -60i64..60 {
            for dy in -60i64..60 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                for sp in [128i64, 256, 384, 768] {
                    let s = dx * dx + dy * dy;
                    let want = ((dx * sp) as f64 / ((s as f64).sqrt())).round_ties_even();
                    let fl = (dx * sp) as f64 / (s as f64).sqrt();
                    let js = (fl + 0.5).floor();
                    let got = round_div_sqrt(dx * sp, s);
                    let _ = want;
                    assert_eq!(got, js as i64, "dx {dx} dy {dy} sp {sp}");
                }
            }
        }
    }
    #[test]
    fn round_div_js() {
        assert_eq!(round_div(5, 2), 3);
        assert_eq!(round_div(-5, 2), -2);
        assert_eq!(round_div(-7, 3), -2);
        assert_eq!(round_div(7, 3), 2);
    }
}
