//! Built-in functions, in one table.
//!
//! The checker reads `params` and `ret` from here, the interpreter calls
//! `func`. Adding a built-in means adding one entry to `BUILTINS`.

mod value;

#[cfg(test)]
mod tests;

pub use value::Value;

use kmpl_ast::Type;

pub struct Builtin {
    pub name: &'static str,
    pub params: &'static [ParamType],
    /// `None` for functions that return nothing.
    pub ret: Option<Type>,
    /// Called with arguments whose types already match `params`.
    pub func: fn(&[Value]) -> Option<Value>,
}

/// What a built-in accepts in one argument position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    Exact(Type),
    /// Any type. Used by `print`.
    Any,
}

const REAL: ParamType = ParamType::Exact(Type::Real);

pub static BUILTINS: &[Builtin] = &[
    real_to_real("sqrt", |a| real_math(a, f64::sqrt)),
    real_to_real("sin", |a| real_math(a, f64::sin)),
    real_to_real("cos", |a| real_math(a, f64::cos)),
    real_to_real("exp", |a| real_math(a, f64::exp)),
    real_to_real("log", |a| real_math(a, f64::ln)),
    real_to_real("abs", |a| real_math(a, f64::abs)),
    Builtin {
        name: "vec2",
        params: &[REAL, REAL],
        ret: Some(Type::Vec2),
        func: |a| Some(Value::Vec2([a[0].as_real(), a[1].as_real()])),
    },
    Builtin {
        name: "vec3",
        params: &[REAL, REAL, REAL],
        ret: Some(Type::Vec3),
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
        params: &[REAL, REAL, REAL, REAL],
        ret: Some(Type::Vec4),
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
        params: &[ParamType::Any],
        ret: None,
        func: |a| {
            println!("{}", a[0]);
            None
        },
    },
];

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

/// Table entry for a `fn(real) -> real` built-in.
const fn real_to_real(name: &'static str, func: fn(&[Value]) -> Option<Value>) -> Builtin {
    Builtin {
        name,
        params: &[REAL],
        ret: Some(Type::Real),
        func,
    }
}

/// Applies `math` (e.g. `f64::sqrt`) to the single `real` argument.
fn real_math(args: &[Value], math: fn(f64) -> f64) -> Option<Value> {
    Some(Value::Real(math(args[0].as_real())))
}
