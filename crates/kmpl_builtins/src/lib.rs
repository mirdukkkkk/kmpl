//! Built-in functions, in one table.
//!
//! The checker reads `generic`, `params` and `ret` from here, the
//! interpreter calls `func`. Adding a built-in means adding one entry
//! to `BUILTINS`.

mod complex;
mod value;

#[cfg(test)]
mod tests;

pub use value::Value;

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
    pub func: fn(&[Value]) -> Option<Value>,
}

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
const REAL_OR_COMPLEX: &[Type] = &[Type::Real, Type::Complex];

pub static BUILTINS: &[Builtin] = &[
    float_fn("sqrt", |a| float_math(a, f64::sqrt, complex::sqrt)),
    float_fn("sin", |a| float_math(a, f64::sin, complex::sin)),
    float_fn("cos", |a| float_math(a, f64::cos, complex::cos)),
    float_fn("exp", |a| float_math(a, f64::exp, complex::exp)),
    float_fn("log", |a| float_math(a, f64::ln, complex::log)),
    Builtin {
        name: "abs",
        generic: &[Type::Int, Type::Real],
        params: &[ParamType::T],
        ret: RetType::T,
        func: |a| match a[0] {
            Value::Int(v) => Some(Value::Int(v.abs())),
            other => Some(Value::Real(other.as_real().abs())),
        },
    },
    Builtin {
        name: "vec2",
        generic: &[],
        params: &[REAL, REAL],
        ret: RetType::Exact(Type::Vec2),
        func: |a| Some(Value::Vec2([a[0].as_real(), a[1].as_real()])),
    },
    Builtin {
        name: "vec3",
        generic: &[],
        params: &[REAL, REAL, REAL],
        ret: RetType::Exact(Type::Vec3),
        func: |a| {
            Some(Value::Vec3([
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
        func: |a| {
            Some(Value::Vec4([
                a[0].as_real(),
                a[1].as_real(),
                a[2].as_real(),
                a[3].as_real(),
            ]))
        },
    },
    Builtin {
        name: "print",
        generic: &[],
        params: &[ParamType::Any],
        ret: RetType::Nothing,
        func: |a| {
            println!("{}", a[0]);
            None
        },
    },
];

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

/// Table entry for a `fn(T) -> T` built-in with `T` in `[real, complex]`.
const fn float_fn(name: &'static str, func: fn(&[Value]) -> Option<Value>) -> Builtin {
    Builtin {
        name,
        generic: REAL_OR_COMPLEX,
        params: &[ParamType::T],
        ret: RetType::T,
        func,
    }
}

/// Applies `real` to a `real` argument or `complex` to a `complex` one.
fn float_math(
    args: &[Value],
    real: fn(f64) -> f64,
    complex: fn(f64, f64) -> (f64, f64),
) -> Option<Value> {
    match args[0] {
        Value::Complex { re, im } => {
            let (re, im) = complex(re, im);
            Some(Value::Complex { re, im })
        }
        other => Some(Value::Real(real(other.as_real()))),
    }
}
