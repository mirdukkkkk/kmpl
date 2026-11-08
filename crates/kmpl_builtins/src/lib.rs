//! Built-in functions, in one table.
//!
//! The checker reads `generic`, `params` and `ret` from here, the
//! interpreter calls `func`. Adding a built-in means adding one entry
//! to `BUILTINS`.

mod complex;
mod math;
mod value;

#[cfg(test)]
mod tests;

pub use value::Value;

use std::io::Write;

use kmpl_ast::Type;

pub struct Builtin {
    pub name: &'static str,
    /// Types the generic parameter `T` may stand for. Empty if the
    /// built-in does not use `T`. All `T` positions in one call are the
    /// same type: `sqrt(T) -> T` with `T` in `[real, complex]`.
    pub generic: &'static [Type],
    pub params: &'static [ParamType],
    pub ret: RetType,
    /// Called with arguments whose types already match `params`.
    /// `out` is where `print` writes. Returns the value (`None` for
    /// `RetType::Nothing`) or a runtime error message.
    pub func: BuiltinFn,
}

pub type BuiltinFn = fn(&[Value], &mut dyn Write) -> Result<Option<Value>, String>;

/// What a built-in accepts in one argument position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    Exact(Type),
    /// The generic type `T`, see `Builtin::generic`.
    T,
    /// Any type. Used by `print`.
    Any,
}

/// What a built-in returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetType {
    Nothing,
    Exact(Type),
    /// The generic type `T`, see `Builtin::generic`.
    T,
}

const REAL: ParamType = ParamType::Exact(Type::Real);
const COMPLEX: ParamType = ParamType::Exact(Type::Complex);
const REAL_OR_COMPLEX: &[Type] = &[Type::Real, Type::Complex];
const INT_OR_REAL: &[Type] = &[Type::Int, Type::Real];
const NUMBER: &[Type] = &[Type::Int, Type::Real, Type::Complex];

pub static BUILTINS: &[Builtin] = &[
    // ----- `T -> T` with `T` in `real, complex` -----
    float_fn("sqrt", |a, _| float_math(a, f64::sqrt, complex::sqrt)),
    float_fn("exp", |a, _| float_math(a, f64::exp, complex::exp)),
    float_fn("log", |a, _| float_math(a, f64::ln, complex::log)),
    float_fn("sin", |a, _| float_math(a, f64::sin, complex::sin)),
    float_fn("cos", |a, _| float_math(a, f64::cos, complex::cos)),
    float_fn("tan", |a, _| float_math(a, f64::tan, complex::tan)),
    float_fn("asin", |a, _| float_math(a, f64::asin, complex::asin)),
    float_fn("acos", |a, _| float_math(a, f64::acos, complex::acos)),
    float_fn("atan", |a, _| float_math(a, f64::atan, complex::atan)),
    float_fn("sinh", |a, _| float_math(a, f64::sinh, complex::sinh)),
    float_fn("cosh", |a, _| float_math(a, f64::cosh, complex::cosh)),
    float_fn("tanh", |a, _| float_math(a, f64::tanh, complex::tanh)),
    // ----- other numbers -----
    Builtin {
        name: "abs",
        generic: INT_OR_REAL,
        params: &[ParamType::T],
        ret: RetType::T,
        func: |a, _| match a[0] {
            Value::Int(n) => math::int_abs(n).map(|n| Some(Value::Int(n))),
            other => ok(Value::Real(other.as_real().abs())),
        },
    },
    Builtin {
        name: "pow",
        generic: NUMBER,
        params: &[ParamType::T, ParamType::T],
        ret: RetType::T,
        func: |a, _| match (a[0], a[1]) {
            (Value::Int(b), Value::Int(e)) => math::int_pow(b, e).map(|n| Some(Value::Int(n))),
            (Value::Complex { re, im }, Value::Complex { re: yr, im: yi }) => {
                let (re, im) = complex::pow((re, im), (yr, yi));
                ok(Value::Complex { re, im })
            }
            (x, y) => ok(Value::Real(x.as_real().powf(y.as_real()))),
        },
    },
    real_to_real("cbrt", |a, _| real_math(a, f64::cbrt)),
    real_to_real("floor", |a, _| real_math(a, f64::floor)),
    real_to_real("ceil", |a, _| real_math(a, f64::ceil)),
    // Halves round away from zero: `round(2.5) = 3`, `round(-2.5) = -3`.
    real_to_real("round", |a, _| real_math(a, f64::round)),
    Builtin {
        name: "atan2",
        generic: &[],
        params: &[REAL, REAL],
        ret: RetType::Exact(Type::Real),
        func: |a, _| ok(Value::Real(a[0].as_real().atan2(a[1].as_real()))),
    },
    Builtin {
        name: "hypot",
        generic: &[],
        params: &[REAL, REAL],
        ret: RetType::Exact(Type::Real),
        func: |a, _| ok(Value::Real(a[0].as_real().hypot(a[1].as_real()))),
    },
    Builtin {
        name: "is_nan",
        generic: &[],
        params: &[REAL],
        ret: RetType::Exact(Type::Bool),
        func: |a, _| ok(Value::Bool(a[0].as_real().is_nan())),
    },
    Builtin {
        name: "is_inf",
        generic: &[],
        params: &[REAL],
        ret: RetType::Exact(Type::Bool),
        func: |a, _| ok(Value::Bool(a[0].as_real().is_infinite())),
    },
    // ----- conversions -----
    Builtin {
        name: "real",
        generic: INT_OR_REAL,
        params: &[ParamType::T],
        ret: RetType::Exact(Type::Real),
        func: |a, _| match a[0] {
            Value::Int(n) => ok(Value::Real(n as f64)),
            other => ok(Value::Real(other.as_real())),
        },
    },
    Builtin {
        name: "int",
        generic: INT_OR_REAL,
        params: &[ParamType::T],
        ret: RetType::Exact(Type::Int),
        func: |a, _| match a[0] {
            Value::Int(n) => ok(Value::Int(n)),
            other => math::real_to_int(other.as_real()).map(|n| Some(Value::Int(n))),
        },
    },
    Builtin {
        name: "complex",
        generic: &[],
        params: &[REAL, REAL],
        ret: RetType::Exact(Type::Complex),
        func: |a, _| {
            ok(Value::Complex {
                re: a[0].as_real(),
                im: a[1].as_real(),
            })
        },
    },
    Builtin {
        name: "re",
        generic: &[],
        params: &[COMPLEX],
        ret: RetType::Exact(Type::Real),
        func: |a, _| ok(Value::Real(a[0].as_complex().0)),
    },
    Builtin {
        name: "im",
        generic: &[],
        params: &[COMPLEX],
        ret: RetType::Exact(Type::Real),
        func: |a, _| ok(Value::Real(a[0].as_complex().1)),
    },
    // ----- vectors -----
    Builtin {
        name: "vec2",
        generic: &[],
        params: &[REAL, REAL],
        ret: RetType::Exact(Type::Vec2),
        func: |a, _| ok(Value::Vec2([a[0].as_real(), a[1].as_real()])),
    },
    Builtin {
        name: "vec3",
        generic: &[],
        params: &[REAL, REAL, REAL],
        ret: RetType::Exact(Type::Vec3),
        func: |a, _| {
            ok(Value::Vec3([
                a[0].as_real(),
                a[1].as_real(),
                a[2].as_real(),
            ]))
        },
    },
    Builtin {
        name: "vec4",
        generic: &[],
        params: &[REAL, REAL, REAL, REAL],
        ret: RetType::Exact(Type::Vec4),
        func: |a, _| {
            ok(Value::Vec4([
                a[0].as_real(),
                a[1].as_real(),
                a[2].as_real(),
                a[3].as_real(),
            ]))
        },
    },
    // ----- output -----
    Builtin {
        name: "print",
        generic: &[],
        params: &[ParamType::Any],
        ret: RetType::Nothing,
        func: |a, out| {
            writeln!(out, "{}", a[0]).map_err(|e| format!("cannot write output: {e}"))?;
            Ok(None)
        },
    },
];

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

/// `Ok(Some(value))`, the result of a built-in that cannot fail.
fn ok(value: Value) -> Result<Option<Value>, String> {
    Ok(Some(value))
}

/// Table entry for a `fn(T) -> T` built-in with `T` in `[real, complex]`.
const fn float_fn(name: &'static str, func: BuiltinFn) -> Builtin {
    Builtin {
        name,
        generic: REAL_OR_COMPLEX,
        params: &[ParamType::T],
        ret: RetType::T,
        func,
    }
}

/// Table entry for a `fn(real) -> real` built-in.
const fn real_to_real(name: &'static str, func: BuiltinFn) -> Builtin {
    Builtin {
        name,
        generic: &[],
        params: &[REAL],
        ret: RetType::Exact(Type::Real),
        func,
    }
}

/// Applies `real` to a `real` argument or `complex` to a `complex` one.
fn float_math(
    args: &[Value],
    real: fn(f64) -> f64,
    complex: fn(complex::C) -> complex::C,
) -> Result<Option<Value>, String> {
    match args[0] {
        Value::Complex { re, im } => {
            let (re, im) = complex((re, im));
            ok(Value::Complex { re, im })
        }
        other => ok(Value::Real(real(other.as_real()))),
    }
}

/// Applies `math` to the single `real` argument.
fn real_math(args: &[Value], math: fn(f64) -> f64) -> Result<Option<Value>, String> {
    ok(Value::Real(math(args[0].as_real())))
}
