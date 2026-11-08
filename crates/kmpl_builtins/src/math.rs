//! Helpers for built-ins that can fail or need more than one line.

use crate::Value;

/// `int(x)`: drops the fractional part (`-2.7` becomes `-2`). Fails instead
/// of saturating like Rust's `as` when the result does not fit.
pub fn real_to_int(x: f64) -> Result<i64, String> {
    let t = x.trunc();
    // 2^63 is exactly representable; every real in [-2^63, 2^63) fits.
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if x.is_nan() {
        Err("cannot convert NaN to `int`".to_owned())
    } else if !(-LIMIT..LIMIT).contains(&t) {
        Err(format!("`{x:?}` is out of range for `int`"))
    } else {
        Ok(t as i64)
    }
}

pub fn int_abs(n: i64) -> Result<i64, String> {
    n.checked_abs().ok_or_else(|| "integer overflow".to_owned())
}

/// `pow(base, exp)` on `int`. A negative exponent has no `int` result.
pub fn int_pow(base: i64, exp: i64) -> Result<i64, String> {
    if exp < 0 {
        return Err("negative exponent in `pow` on `int`, use `real` instead".to_owned());
    }
    let overflow = || "integer overflow".to_owned();
    match u32::try_from(exp) {
        Ok(e) => base.checked_pow(e).ok_or_else(overflow),
        // Huge exponents only fit for these bases.
        Err(_) => match base {
            0 | 1 => Ok(base),
            -1 => Ok(if exp % 2 == 0 { 1 } else { -1 }),
            _ => Err(overflow()),
        },
    }
}

/// `clamp(x, lo, hi)` on `int` or `real`. `lo > hi` is an error instead of
/// a panic (Rust's `clamp` panics). A NaN `x` stays NaN.
pub fn clamp(x: Value, lo: Value, hi: Value) -> Result<Value, String> {
    let bad_bounds = || format!("`clamp` needs lower bound <= upper bound, got {lo} and {hi}");
    match (&x, &lo, &hi) {
        (Value::Int(x), Value::Int(lo), Value::Int(hi)) => {
            if lo > hi {
                return Err(bad_bounds());
            }
            Ok(Value::Int(*x.max(lo).min(hi)))
        }
        (x, lo, hi) => {
            let (x, lo, hi) = (x.as_real(), lo.as_real(), hi.as_real());
            if lo > hi {
                return Err(bad_bounds());
            }
            let v = if x < lo {
                lo
            } else if x > hi {
                hi
            } else {
                x
            };
            Ok(Value::Real(v))
        }
    }
}

/// `a + (b - a) * t` for `real`, `complex` and vectors.
pub fn lerp(a: Value, b: Value, t: f64) -> Value {
    let mix = |x: f64, y: f64| x + (y - x) * t;
    match (a, b) {
        (Value::Real(x), Value::Real(y)) => Value::Real(mix(x, y)),
        (Value::Complex { re: ar, im: ai }, Value::Complex { re: br, im: bi }) => Value::Complex {
            re: mix(ar, br),
            im: mix(ai, bi),
        },
        (a, b) => a.zip_components(&b, mix).expect("same vector type"),
    }
}

/// Hermite step from 0 at `e0` to 1 at `e1`, as in GLSL. With `e0 == e1`
/// it is a plain step instead of `0 / 0`.
pub fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    if e0 == e1 {
        return if x < e0 { 0.0 } else { 1.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn dot(a: &Value, b: &Value) -> f64 {
    a.components()
        .iter()
        .zip(b.components())
        .map(|(x, y)| x * y)
        .sum()
}
