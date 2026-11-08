use std::collections::HashSet;

use kmpl_ast::Type;

use crate::{BUILTINS, ParamType, Value, lookup};

/// Some value accepted by the given parameter, to call built-ins with.
fn sample(param: ParamType) -> Value {
    let ty = match param {
        ParamType::Exact(ty) => ty,
        ParamType::Any => Type::Int,
    };

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
    (lookup(name).unwrap().func)(args)
}

#[test]
fn names_are_unique() {
    let mut seen = HashSet::new();
    for b in BUILTINS {
        assert!(seen.insert(b.name), "duplicate built-in `{}`", b.name);
    }
}

/// Every built-in accepts arguments of its declared `params`
/// and returns a value of its declared `ret`.
#[test]
fn signatures_match_implementations() {
    for b in BUILTINS {
        let args: Vec<Value> = b.params.iter().map(|&p| sample(p)).collect();
        let result = (b.func)(&args);
        assert_eq!(result.map(|v| v.ty()), b.ret, "built-in `{}`", b.name);
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
    assert_eq!(print.ret, None);
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
