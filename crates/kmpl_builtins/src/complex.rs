//! Complex math on `(re, im)` pairs. Principal branches, as in most
//! math libraries: `sqrt(-1) = i`, `log(-1) = πi`.

/// A complex number as `(re, im)`.
pub type C = (f64, f64);

const I: C = (0.0, 1.0);
const ONE: C = (1.0, 0.0);

fn add(a: C, b: C) -> C {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: C, b: C) -> C {
    (a.0 - b.0, a.1 - b.1)
}

fn mul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

fn div(a: C, b: C) -> C {
    let den = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / den, (a.1 * b.0 - a.0 * b.1) / den)
}

pub fn sqrt((re, im): C) -> C {
    let r = re.hypot(im);
    let out_re = ((r + re) / 2.0).sqrt();
    // `copysign` keeps the sign of `im`, so the result lies in the
    // right half-plane and `sqrt(-1 + 0i)` is `+i`.
    let out_im = ((r - re) / 2.0).sqrt().copysign(im);
    (out_re, out_im)
}

pub fn exp((re, im): C) -> C {
    let scale = re.exp();
    (scale * im.cos(), scale * im.sin())
}

pub fn log((re, im): C) -> C {
    (re.hypot(im).ln(), im.atan2(re))
}

pub fn sin((re, im): C) -> C {
    (re.sin() * im.cosh(), re.cos() * im.sinh())
}

pub fn cos((re, im): C) -> C {
    (re.cos() * im.cosh(), -re.sin() * im.sinh())
}

pub fn tan(z: C) -> C {
    div(sin(z), cos(z))
}

pub fn sinh((re, im): C) -> C {
    (re.sinh() * im.cos(), re.cosh() * im.sin())
}

pub fn cosh((re, im): C) -> C {
    (re.cosh() * im.cos(), re.sinh() * im.sin())
}

pub fn tanh(z: C) -> C {
    div(sinh(z), cosh(z))
}

/// `asin(z) = -i log(iz + sqrt(1 - z²))`
pub fn asin(z: C) -> C {
    let inner = add(mul(I, z), sqrt(sub(ONE, mul(z, z))));
    mul((0.0, -1.0), log(inner))
}

/// `acos(z) = π/2 - asin(z)`
pub fn acos(z: C) -> C {
    sub((std::f64::consts::FRAC_PI_2, 0.0), asin(z))
}

/// `atan(z) = (i/2) (log(1 - iz) - log(1 + iz))`
pub fn atan(z: C) -> C {
    let iz = mul(I, z);
    let diff = sub(log(sub(ONE, iz)), log(add(ONE, iz)));
    mul((0.0, 0.5), diff)
}

/// `pow(x, y) = exp(y log x)`, with `pow(0, 0) = 1` and `pow(0, y) = 0`.
pub fn pow(x: C, y: C) -> C {
    if x == (0.0, 0.0) {
        return if y == (0.0, 0.0) { ONE } else { (0.0, 0.0) };
    }
    exp(mul(y, log(x)))
}
