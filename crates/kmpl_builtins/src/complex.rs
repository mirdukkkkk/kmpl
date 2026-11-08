//! Complex math on `(re, im)` pairs. Principal branches, as in most
//! math libraries: `sqrt(-1) = i`, `log(-1) = πi`.

pub fn sqrt(re: f64, im: f64) -> (f64, f64) {
    let r = re.hypot(im);
    let out_re = ((r + re) / 2.0).sqrt();
    // `copysign` keeps the sign of `im`, so the result lies in the
    // right half-plane and `sqrt(-1 + 0i)` is `+i`.
    let out_im = ((r - re) / 2.0).sqrt().copysign(im);
    (out_re, out_im)
}

pub fn exp(re: f64, im: f64) -> (f64, f64) {
    let scale = re.exp();
    (scale * im.cos(), scale * im.sin())
}

pub fn log(re: f64, im: f64) -> (f64, f64) {
    (re.hypot(im).ln(), im.atan2(re))
}

pub fn sin(re: f64, im: f64) -> (f64, f64) {
    (re.sin() * im.cosh(), re.cos() * im.sinh())
}

pub fn cos(re: f64, im: f64) -> (f64, f64) {
    (re.cos() * im.cosh(), -re.sin() * im.sinh())
}
