//! `int` and `real` helpers for built-ins that can fail or need care.

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
