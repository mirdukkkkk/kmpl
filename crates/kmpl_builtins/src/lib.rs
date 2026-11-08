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
    pub params: &'static [Type],
    /// `None` for functions that return nothing.
    pub ret: Option<Type>,
    /// Called with arguments whose types already match `params`.
    pub func: fn(&[Value]) -> Option<Value>,
}

pub static BUILTINS: &[Builtin] = &[
    real_to_real("sqrt", |a| real_math(a, f32::sqrt)),
    real_to_real("sin", |a| real_math(a, f32::sin)),
    real_to_real("cos", |a| real_math(a, f32::cos)),
    real_to_real("exp", |a| real_math(a, f32::exp)),
    real_to_real("log", |a| real_math(a, f32::ln)),
    real_to_real("abs", |a| real_math(a, f32::abs)),
    Builtin {
        name: "vec2",
        params: &[Type::Real, Type::Real],
        ret: Some(Type::Vec2),
        func: |a| Some(Value::Vec2([a[0].as_real(), a[1].as_real()])),
    },
    Builtin {
        name: "vec3",
        params: &[Type::Real, Type::Real, Type::Real],
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
        params: &[Type::Real, Type::Real, Type::Real, Type::Real],
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
];

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

/// Table entry for a `fn(real) -> real` built-in.
const fn real_to_real(name: &'static str, func: fn(&[Value]) -> Option<Value>) -> Builtin {
    Builtin {
        name,
        params: &[Type::Real],
        ret: Some(Type::Real),
        func,
    }
}

/// Applies `math` (e.g. `f32::sqrt`) to the single `real` argument.
fn real_math(args: &[Value], math: fn(f32) -> f32) -> Option<Value> {
    Some(Value::Real(math(args[0].as_real())))
}
