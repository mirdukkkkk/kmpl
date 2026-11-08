use crate::Value;

use super::call;

/// Calls a one-argument built-in on `z` and returns `(re, im)`.
fn f(name: &str, z: (f64, f64)) -> (f64, f64) {
    pair(call(name, &[c(z)]), name)
}

fn c((re, im): (f64, f64)) -> Value {
    Value::Complex { re, im }
}

fn pair(v: Option<Value>, name: &str) -> (f64, f64) {
    match v {
        Some(Value::Complex { re, im }) => (re, im),
        other => panic!("`{name}` returned {other:?}"),
    }
}

fn assert_close(actual: (f64, f64), expected: (f64, f64)) {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(
        close(actual.0, expected.0) && close(actual.1, expected.1),
        "{actual:?} != {expected:?}"
    );
}

const SAMPLES: [(f64, f64); 5] = [
    (0.5, 0.25),
    (-0.3, 0.7),
    (1.5, -0.5),
    (0.0, 2.0),
    (-2.0, -1.0),
];

#[test]
fn inverse_functions_undo_their_partner() {
    for z in SAMPLES {
        assert_close(f("sin", f("asin", z)), z);
        assert_close(f("cos", f("acos", z)), z);
        assert_close(f("tan", f("atan", z)), z);
        assert_close(f("exp", f("log", z)), z);
    }
}

#[test]
fn hyperbolic_identities() {
    for z in SAMPLES {
        let (s, ch) = (f("sinh", z), f("cosh", z));
        // cosh² - sinh² = 1
        let sq = |(a, b): (f64, f64)| (a * a - b * b, 2.0 * a * b);
        let (c2, s2) = (sq(ch), sq(s));
        assert_close((c2.0 - s2.0, c2.1 - s2.1), (1.0, 0.0));

        // tanh = sinh / cosh, checked as tanh * cosh = sinh
        let t = f("tanh", z);
        assert_close((t.0 * ch.0 - t.1 * ch.1, t.0 * ch.1 + t.1 * ch.0), s);
    }
}

#[test]
fn real_inputs_match_real_functions() {
    for name in ["tan", "sinh", "cosh", "tanh", "atan"] {
        let (re, im) = f(name, (0.7, 0.0));
        let Some(Value::Real(expected)) = call(name, &[Value::Real(0.7)]) else {
            panic!("`{name}` on real");
        };
        assert_close((re, im), (expected, 0.0));
    }
}

#[test]
fn complex_pow() {
    let pow = |x, y| pair(call("pow", &[c(x), c(y)]), "pow");
    assert_close(pow((0.0, 1.0), (2.0, 0.0)), (-1.0, 0.0));
    // e^(iπ) = -1
    assert_close(
        pow((std::f64::consts::E, 0.0), (0.0, std::f64::consts::PI)),
        (-1.0, 0.0),
    );
    assert_close(pow((-1.0, 0.0), (0.5, 0.0)), (0.0, 1.0));
    assert_eq!(pow((0.0, 0.0), (0.0, 0.0)), (1.0, 0.0));
    assert_eq!(pow((0.0, 0.0), (2.0, 0.0)), (0.0, 0.0));
}

#[test]
fn modulus_argument_and_conjugate() {
    assert_eq!(call("abs", &[c((3.0, -4.0))]), Some(Value::Real(5.0)));
    assert_eq!(
        call("arg", &[c((0.0, 1.0))]),
        Some(Value::Real(std::f64::consts::FRAC_PI_2))
    );
    assert_eq!(
        call("arg", &[c((-1.0, 0.0))]),
        Some(Value::Real(std::f64::consts::PI))
    );
    assert_eq!(f("conj", (1.0, 2.0)), (1.0, -2.0));
    // No negative zero: conj(2 + 0i) is 2 + 0i, as for negation.
    assert!(f("conj", (2.0, 0.0)).1.is_sign_positive());
}
