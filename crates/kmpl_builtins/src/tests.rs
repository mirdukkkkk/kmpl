use std::collections::HashSet;

use kmpl_ast::Type;

use crate::{BUILTINS, Value, lookup};

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
        let args: Vec<Value> = b.params.iter().map(|&ty| sample(ty)).collect();
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
