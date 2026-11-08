use crate::Value::{self, Complex, Int, Real, Vec2, Vec3, Vec4};
use crate::lookup;

use super::call;

fn error(name: &str, args: &[Value]) -> String {
    (lookup(name).unwrap().func)(args, &mut Vec::new()).unwrap_err()
}

#[test]
fn dot_length_distance() {
    assert_eq!(
        call("dot", &[Vec3([1.0, 2.0, 3.0]), Vec3([4.0, -5.0, 6.0])]),
        Some(Real(12.0))
    );
    assert_eq!(call("length", &[Vec2([3.0, 4.0])]), Some(Real(5.0)));
    assert_eq!(
        call("length", &[Vec4([1.0, 1.0, 1.0, 1.0])]),
        Some(Real(2.0))
    );
    assert_eq!(
        call("distance", &[Vec2([1.0, 1.0]), Vec2([4.0, 5.0])]),
        Some(Real(5.0))
    );
    assert_eq!(call("distance", &[Real(-1.0), Real(2.5)]), Some(Real(3.5)));
}

#[test]
fn normalize() {
    assert_eq!(
        call("normalize", &[Vec2([3.0, 4.0])]),
        Some(Vec2([0.6, 0.8]))
    );
    let Some(Vec3(zero)) = call("normalize", &[Vec3([0.0; 3])]) else {
        panic!("expected vec3");
    };
    assert!(zero.iter().all(|c| c.is_nan()));
}

#[test]
fn cross_product() {
    let x = Vec3([1.0, 0.0, 0.0]);
    let y = Vec3([0.0, 1.0, 0.0]);
    assert_eq!(
        call("cross", &[x.clone(), y.clone()]),
        Some(Vec3([0.0, 0.0, 1.0]))
    );
    assert_eq!(call("cross", &[y, x]), Some(Vec3([0.0, 0.0, -1.0])));
}

#[test]
fn min_max() {
    assert_eq!(call("min", &[Int(3), Int(-2)]), Some(Int(-2)));
    assert_eq!(call("max", &[Int(3), Int(-2)]), Some(Int(3)));
    assert_eq!(call("min", &[Real(0.5), Real(0.25)]), Some(Real(0.25)));
    assert_eq!(call("max", &[Real(f64::NAN), Real(1.0)]), Some(Real(1.0)));
}

#[test]
fn clamp() {
    assert_eq!(call("clamp", &[Int(5), Int(0), Int(3)]), Some(Int(3)));
    assert_eq!(call("clamp", &[Int(-5), Int(0), Int(3)]), Some(Int(0)));
    assert_eq!(
        call("clamp", &[Real(0.5), Real(0.0), Real(1.0)]),
        Some(Real(0.5))
    );
    assert_eq!(
        error("clamp", &[Int(1), Int(3), Int(0)]),
        "`clamp` needs lower bound <= upper bound, got 3 and 0"
    );
    assert_eq!(
        error("clamp", &[Real(1.0), Real(2.0), Real(1.5)]),
        "`clamp` needs lower bound <= upper bound, got 2.0 and 1.5"
    );
    let Some(Real(nan)) = call("clamp", &[Real(f64::NAN), Real(0.0), Real(1.0)]) else {
        panic!("expected real");
    };
    assert!(nan.is_nan());
}

#[test]
fn lerp_on_every_type() {
    assert_eq!(
        call("lerp", &[Real(0.0), Real(10.0), Real(0.25)]),
        Some(Real(2.5))
    );
    assert_eq!(
        call("lerp", &[Vec2([0.0, 0.0]), Vec2([2.0, 4.0]), Real(0.5)]),
        Some(Vec2([1.0, 2.0]))
    );
    assert_eq!(
        call(
            "lerp",
            &[
                Complex { re: 0.0, im: 0.0 },
                Complex { re: 2.0, im: -2.0 },
                Real(0.5)
            ]
        ),
        Some(Complex { re: 1.0, im: -1.0 })
    );
}

#[test]
fn smoothstep() {
    let s = |e0, e1, x| call("smoothstep", &[Real(e0), Real(e1), Real(x)]);
    assert_eq!(s(0.0, 1.0, -1.0), Some(Real(0.0)));
    assert_eq!(s(0.0, 1.0, 0.5), Some(Real(0.5)));
    assert_eq!(s(0.0, 1.0, 2.0), Some(Real(1.0)));
    assert_eq!(s(0.0, 2.0, 0.5), Some(Real(0.15625)));
    assert_eq!(s(1.0, 1.0, 0.5), Some(Real(0.0)));
    assert_eq!(s(1.0, 1.0, 1.0), Some(Real(1.0)));
}
