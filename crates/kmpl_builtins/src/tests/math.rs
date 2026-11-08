use crate::Value::{self, Bool, Int, Real};
use crate::lookup;

use super::call;

fn error(name: &str, args: &[Value]) -> String {
    (lookup(name).unwrap().func)(args, &mut Vec::new()).unwrap_err()
}

fn real(name: &str, args: &[f64]) -> f64 {
    let args: Vec<Value> = args.iter().map(|&x| Real(x)).collect();
    match call(name, &args) {
        Some(Real(x)) => x,
        other => panic!("`{name}` returned {other:?}"),
    }
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
}

#[test]
fn trig_and_hyperbolic_on_reals() {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    assert_close(real("tan", &[FRAC_PI_4]), 1.0);
    assert_close(real("asin", &[1.0]), FRAC_PI_2);
    assert_close(real("acos", &[-1.0]), PI);
    assert_close(real("atan", &[1.0]), FRAC_PI_4);
    assert_close(real("sinh", &[0.0]), 0.0);
    assert_close(real("cosh", &[0.0]), 1.0);
    assert_close(real("tanh", &[1.0]), 1f64.sinh() / 1f64.cosh());
    assert!(real("asin", &[2.0]).is_nan());
}

#[test]
fn atan2_and_hypot() {
    use std::f64::consts::PI;

    assert_close(real("atan2", &[1.0, -1.0]), 3.0 * PI / 4.0);
    assert_close(real("atan2", &[-1.0, 0.0]), -PI / 2.0);
    assert_eq!(real("hypot", &[3.0, 4.0]), 5.0);
}

#[test]
fn rounding() {
    assert_eq!(real("floor", &[-1.5]), -2.0);
    assert_eq!(real("ceil", &[-1.5]), -1.0);
    assert_eq!(real("round", &[2.5]), 3.0);
    assert_eq!(real("round", &[-2.5]), -3.0);
    assert_eq!(real("round", &[2.4]), 2.0);
    assert_eq!(real("cbrt", &[-8.0]), -2.0);
}

#[test]
fn nan_and_inf_checks() {
    assert_eq!(call("is_nan", &[Real(f64::NAN)]), Some(Bool(true)));
    assert_eq!(call("is_nan", &[Real(1.0)]), Some(Bool(false)));
    assert_eq!(call("is_inf", &[Real(-f64::INFINITY)]), Some(Bool(true)));
    assert_eq!(call("is_inf", &[Real(f64::NAN)]), Some(Bool(false)));
}

#[test]
fn pow_on_each_type() {
    assert_eq!(call("pow", &[Int(2), Int(10)]), Some(Int(1024)));
    assert_eq!(call("pow", &[Int(-3), Int(3)]), Some(Int(-27)));
    assert_eq!(call("pow", &[Int(5), Int(0)]), Some(Int(1)));
    assert_eq!(real("pow", &[2.0, 0.5]), 2f64.sqrt());
    assert_eq!(real("pow", &[2.0, -1.0]), 0.5);
}

#[test]
fn int_pow_errors_and_huge_exponents() {
    assert_eq!(
        error("pow", &[Int(2), Int(-1)]),
        "negative exponent in `pow` on `int`, use `real` instead"
    );
    assert_eq!(error("pow", &[Int(2), Int(64)]), "integer overflow");
    let huge = i64::MAX;
    assert_eq!(call("pow", &[Int(1), Int(huge)]), Some(Int(1)));
    assert_eq!(call("pow", &[Int(0), Int(huge)]), Some(Int(0)));
    assert_eq!(call("pow", &[Int(-1), Int(huge)]), Some(Int(-1)));
    assert_eq!(call("pow", &[Int(-1), Int(huge - 1)]), Some(Int(1)));
    assert_eq!(error("pow", &[Int(2), Int(huge)]), "integer overflow");
}

#[test]
fn abs_of_int_min_is_an_error() {
    assert_eq!(error("abs", &[Int(i64::MIN)]), "integer overflow");
    assert_eq!(call("abs", &[Int(i64::MIN + 1)]), Some(Int(i64::MAX)));
}
