use std::collections::HashSet;

use kmpl_ast::Type;

use crate::{BUILTINS, ParamType, RetType, Value, lookup};

/// Some value of the given type, to call built-ins with.
fn sample(ty: Type) -> Value {
    match ty {
        Type::Int => Value::Int(2),
        Type::Real => Value::Real(2.0),
        Type::Bool => Value::Bool(true),
        Type::Complex => Value::Complex { re: 2.0, im: 1.0 },
        Type::Vec2 => Value::Vec2([1.0, 2.0]),
        Type::Vec3 => Value::Vec3([1.0, 2.0, 3.0]),
        Type::Vec4 => Value::Vec4([1.0, 2.0, 3.0, 4.0]),
    }
}

fn call(name: &str, args: &[Value]) -> Option<Value> {
    (lookup(name).unwrap().func)(args, &mut Vec::new()).unwrap()
}

#[test]
fn names_are_unique() {
    let mut seen = HashSet::new();
    for b in BUILTINS {
        assert!(seen.insert(b.name), "duplicate built-in `{}`", b.name);
    }
}

/// Every built-in accepts arguments of its declared `params` and returns
/// a value of its declared `ret`, for every type its `T` may stand for.
#[test]
fn signatures_match_implementations() {
    for b in BUILTINS {
        // Non-generic built-ins are called once; `T` is unused then.
        let choices: &[Type] = if b.generic.is_empty() {
            &[Type::Int]
        } else {
            b.generic
        };

        for &t in choices {
            let args: Vec<Value> = b
                .params
                .iter()
                .map(|p| match p {
                    ParamType::Exact(ty) => sample(*ty),
                    ParamType::T => sample(t),
                    ParamType::Any => sample(Type::Int),
                })
                .collect();

            let expected = match b.ret {
                RetType::Nothing => None,
                RetType::Exact(ty) => Some(ty),
                RetType::T => Some(t),
            };

            let result = (b.func)(&args, &mut Vec::new()).unwrap();
            assert_eq!(
                result.map(|v| v.ty()),
                expected,
                "built-in `{}` with T = {t:?}",
                b.name
            );
        }
    }
}

#[test]
fn math() {
    assert_eq!(call("sqrt", &[Value::Real(9.0)]), Some(Value::Real(3.0)));
    assert_eq!(call("abs", &[Value::Real(-1.5)]), Some(Value::Real(1.5)));
    assert_eq!(call("exp", &[Value::Real(0.0)]), Some(Value::Real(1.0)));
    assert_eq!(call("log", &[Value::Real(1.0)]), Some(Value::Real(0.0)));
    assert_eq!(call("sin", &[Value::Real(0.0)]), Some(Value::Real(0.0)));
    assert_eq!(call("cos", &[Value::Real(0.0)]), Some(Value::Real(1.0)));
}

#[test]
fn vector_constructors() {
    let r = |v| Value::Real(v);
    assert_eq!(
        call("vec2", &[r(1.0), r(2.0)]),
        Some(Value::Vec2([1.0, 2.0]))
    );
    assert_eq!(
        call("vec3", &[r(1.0), r(2.0), r(3.0)]),
        Some(Value::Vec3([1.0, 2.0, 3.0]))
    );
    assert_eq!(
        call("vec4", &[r(1.0), r(2.0), r(3.0), r(4.0)]),
        Some(Value::Vec4([1.0, 2.0, 3.0, 4.0]))
    );
}

#[test]
fn unknown_name() {
    assert!(lookup("lerp").is_none());
}

#[test]
fn print_accepts_any_type() {
    let print = lookup("print").unwrap();
    assert_eq!(print.params, &[ParamType::Any]);
    assert_eq!(print.ret, RetType::Nothing);
}

#[test]
fn display() {
    let show = |v: Value| v.to_string();
    assert_eq!(show(Value::Int(-42)), "-42");
    assert_eq!(show(Value::Real(1.0)), "1.0");
    assert_eq!(show(Value::Real(0.1)), "0.1");
    assert_eq!(show(Value::Bool(false)), "false");
    assert_eq!(show(Value::Complex { re: 2.0, im: 3.0 }), "2.0+3.0i");
    assert_eq!(show(Value::Complex { re: 2.0, im: -3.0 }), "2.0-3.0i");
    assert_eq!(show(Value::Vec2([1.0, -2.5])), "(1.0, -2.5)");
    assert_eq!(show(Value::Vec4([0.0; 4])), "(0.0, 0.0, 0.0, 0.0)");
}

fn complex(re: f64, im: f64) -> Value {
    Value::Complex { re, im }
}

/// Calls a built-in on a complex number and returns `(re, im)`.
fn call_complex(name: &str, re: f64, im: f64) -> (f64, f64) {
    match call(name, &[complex(re, im)]) {
        Some(Value::Complex { re, im }) => (re, im),
        other => panic!("`{name}` returned {other:?}"),
    }
}

fn assert_close(actual: (f64, f64), expected: (f64, f64)) {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
    assert!(
        close(actual.0, expected.0) && close(actual.1, expected.1),
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn sqrt_of_negative_real_is_nan() {
    let Some(Value::Real(v)) = call("sqrt", &[Value::Real(-1.0)]) else {
        panic!("expected a real");
    };
    assert!(v.is_nan());
}

#[test]
fn complex_math() {
    use std::f64::consts::PI;

    assert_close(call_complex("sqrt", -1.0, 0.0), (0.0, 1.0));
    assert_close(call_complex("sqrt", -4.0, 0.0), (0.0, 2.0));
    assert_close(call_complex("sqrt", 0.0, 2.0), (1.0, 1.0));
    assert_close(call_complex("sqrt", 0.0, -2.0), (1.0, -1.0));
    assert_close(call_complex("exp", 0.0, PI), (-1.0, 0.0));
    assert_close(call_complex("log", -1.0, 0.0), (0.0, PI));
    assert_close(call_complex("sin", 0.0, 0.0), (0.0, 0.0));
    assert_close(call_complex("cos", 0.0, 0.0), (1.0, 0.0));
    // sin(i) = i sinh(1), cos(i) = cosh(1)
    assert_close(call_complex("sin", 0.0, 1.0), (0.0, 1f64.sinh()));
    assert_close(call_complex("cos", 0.0, 1.0), (1f64.cosh(), 0.0));
}

#[test]
fn abs_keeps_the_type() {
    assert_eq!(call("abs", &[Value::Int(-3)]), Some(Value::Int(3)));
    assert_eq!(call("abs", &[Value::Real(-2.5)]), Some(Value::Real(2.5)));
}

#[test]
fn print_writes_to_out() {
    let print = lookup("print").unwrap();
    let mut out = Vec::new();
    (print.func)(&[Value::Int(1)], &mut out).unwrap();
    (print.func)(&[Value::Vec2([0.5, 2.0])], &mut out).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "1\n(0.5, 2.0)\n");
}

#[test]
fn print_reports_write_errors() {
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let print = lookup("print").unwrap();
    let err = (print.func)(&[Value::Int(1)], &mut Broken).unwrap_err();
    assert_eq!(err, "cannot write output: disk full");
}
