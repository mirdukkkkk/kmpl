use kmpl_ast::{BinOp, UnOp};
use kmpl_builtins::Value::{self, Bool, Complex, Int, Real, Vec2, Vec3, Vec4};

use crate::ops::{binary, swizzle, unary};

fn bin(op: BinOp, l: Value, r: Value) -> Value {
    binary(op, l, r).unwrap()
}

fn c(re: f64, im: f64) -> Value {
    Complex { re, im }
}

#[test]
fn int_arithmetic() {
    assert_eq!(bin(BinOp::Add, Int(2), Int(3)), Int(5));
    assert_eq!(bin(BinOp::Sub, Int(2), Int(3)), Int(-1));
    assert_eq!(bin(BinOp::Mul, Int(-4), Int(3)), Int(-12));
    assert_eq!(bin(BinOp::Div, Int(7), Int(2)), Int(3));
    assert_eq!(bin(BinOp::Div, Int(-7), Int(2)), Int(-3));
}

#[test]
fn int_errors() {
    assert_eq!(
        binary(BinOp::Div, Int(1), Int(0)),
        Err("division by zero".to_owned())
    );
    assert_eq!(
        binary(BinOp::Add, Int(i64::MAX), Int(1)),
        Err("integer overflow".to_owned())
    );
    assert_eq!(
        binary(BinOp::Mul, Int(i64::MAX), Int(2)),
        Err("integer overflow".to_owned())
    );
    assert_eq!(
        binary(BinOp::Div, Int(i64::MIN), Int(-1)),
        Err("integer overflow".to_owned())
    );
    assert_eq!(
        unary(UnOp::Neg, Int(i64::MIN)),
        Err("integer overflow".to_owned())
    );
}

#[test]
fn real_arithmetic() {
    assert_eq!(bin(BinOp::Add, Real(0.5), Real(0.25)), Real(0.75));
    assert_eq!(bin(BinOp::Div, Real(1.0), Real(4.0)), Real(0.25));
    assert_eq!(bin(BinOp::Div, Real(1.0), Real(0.0)), Real(f64::INFINITY));
    let Real(nan) = bin(BinOp::Div, Real(0.0), Real(0.0)) else {
        panic!("expected real");
    };
    assert!(nan.is_nan());
}

#[test]
fn complex_arithmetic() {
    assert_eq!(bin(BinOp::Add, c(1.0, 2.0), c(3.0, -1.0)), c(4.0, 1.0));
    assert_eq!(bin(BinOp::Sub, c(1.0, 2.0), c(3.0, -1.0)), c(-2.0, 3.0));
    // i * i = -1
    assert_eq!(bin(BinOp::Mul, c(0.0, 1.0), c(0.0, 1.0)), c(-1.0, 0.0));
    // (1 + 2i)(3 + 4i) = -5 + 10i
    assert_eq!(bin(BinOp::Mul, c(1.0, 2.0), c(3.0, 4.0)), c(-5.0, 10.0));
    // (-5 + 10i) / (3 + 4i) = 1 + 2i
    assert_eq!(bin(BinOp::Div, c(-5.0, 10.0), c(3.0, 4.0)), c(1.0, 2.0));
    assert_eq!(unary(UnOp::Neg, c(1.0, -2.0)).unwrap(), c(-1.0, 2.0));
    let Complex { im, .. } = unary(UnOp::Neg, c(1.0, 0.0)).unwrap() else {
        panic!("expected complex");
    };
    assert!(im.is_sign_positive(), "-(1 + 0i) must keep +0");
}

#[test]
fn vector_arithmetic() {
    assert_eq!(
        bin(BinOp::Add, Vec2([1.0, 2.0]), Vec2([3.0, 4.0])),
        Vec2([4.0, 6.0])
    );
    assert_eq!(
        bin(BinOp::Sub, Vec3([1.0, 2.0, 3.0]), Vec3([1.0, 1.0, 1.0])),
        Vec3([0.0, 1.0, 2.0])
    );
    assert_eq!(
        bin(BinOp::Mul, Vec3([1.0, 2.0, 3.0]), Real(2.0)),
        Vec3([2.0, 4.0, 6.0])
    );
    assert_eq!(
        bin(BinOp::Mul, Real(0.5), Vec4([2.0, 4.0, 6.0, 8.0])),
        Vec4([1.0, 2.0, 3.0, 4.0])
    );
    assert_eq!(
        bin(BinOp::Div, Vec2([2.0, 4.0]), Real(2.0)),
        Vec2([1.0, 2.0])
    );
    assert_eq!(
        unary(UnOp::Neg, Vec2([1.0, -2.0])).unwrap(),
        Vec2([-1.0, 2.0])
    );
}

#[test]
fn comparisons() {
    assert_eq!(bin(BinOp::Lt, Int(1), Int(2)), Bool(true));
    assert_eq!(bin(BinOp::Ge, Int(1), Int(2)), Bool(false));
    assert_eq!(bin(BinOp::Le, Real(2.0), Real(2.0)), Bool(true));
    assert_eq!(bin(BinOp::Gt, Real(-1.0), Real(2.0)), Bool(false));
}

#[test]
fn equality_on_every_type() {
    assert_eq!(bin(BinOp::Eq, Int(1), Int(1)), Bool(true));
    assert_eq!(bin(BinOp::Ne, Bool(true), Bool(false)), Bool(true));
    assert_eq!(bin(BinOp::Eq, c(1.0, 2.0), c(1.0, 2.0)), Bool(true));
    assert_eq!(
        bin(BinOp::Eq, Vec2([1.0, 2.0]), Vec2([1.0, 3.0])),
        Bool(false)
    );
    // IEEE 754: NaN is not equal to itself.
    assert_eq!(bin(BinOp::Eq, Real(f64::NAN), Real(f64::NAN)), Bool(false));
}

#[test]
fn not() {
    assert_eq!(unary(UnOp::Not, Bool(true)).unwrap(), Bool(false));
}

#[test]
fn swizzles() {
    let v = Vec4([1.0, 2.0, 3.0, 4.0]);
    assert_eq!(swizzle(v.clone(), "x"), Real(1.0));
    assert_eq!(swizzle(v.clone(), "wz"), Vec2([4.0, 3.0]));
    assert_eq!(swizzle(v.clone(), "xxy"), Vec3([1.0, 1.0, 2.0]));
    assert_eq!(swizzle(v, "wzyx"), Vec4([4.0, 3.0, 2.0, 1.0]));
    assert_eq!(
        swizzle(Vec2([5.0, 6.0]), "yxyx"),
        Vec4([6.0, 5.0, 6.0, 5.0])
    );
}

#[test]
#[should_panic(expected = "the checker should have rejected it")]
fn checker_bugs_panic() {
    let _ = binary(BinOp::Add, Int(1), Real(1.0));
}
