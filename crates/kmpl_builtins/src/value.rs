use std::sync::Arc;

use kmpl_ast::Type;

/// A runtime value. One variant per `Type`, plus arrays and tuples.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Real(f64),
    Bool(bool),
    Complex {
        re: f64,
        im: f64,
    },
    Vec2([f64; 2]),
    Vec3([f64; 3]),
    Vec4([f64; 4]),
    /// Strings cannot be changed in place, so copies just share the text.
    Str(Arc<str>),
    /// Arrays behave like values: `let b = a;` gives an independent array.
    /// Copying only shares the `Arc`; a write copies the elements if the
    /// array is shared (`Arc::make_mut`). `Arc` rather than `Rc` because
    /// `Value` is stored in the `static` table of built-in constants.
    Array(Arc<Vec<Value>>),
    /// Never changed after it is built, so it is simply shared.
    Tuple(Arc<[Value]>),
}

impl Value {
    /// The type of this value. Panics on an array or tuple, which have no
    /// `Type`: only built-in constants and tests ask, and those are never
    /// arrays or tuples.
    pub fn ty(&self) -> Type {
        match self {
            Value::Int(_) => Type::Int,
            Value::Real(_) => Type::Real,
            Value::Bool(_) => Type::Bool,
            Value::Complex { .. } => Type::Complex,
            Value::Vec2(_) => Type::Vec2,
            Value::Vec3(_) => Type::Vec3,
            Value::Vec4(_) => Type::Vec4,
            Value::Str(_) => Type::Str,
            Value::Array(_) => panic!("an array has no `Type`"),
            Value::Tuple(_) => panic!("a tuple has no `Type`"),
        }
    }

    /// The inner `f64` of a `Real`.
    ///
    /// Panics on other variants. The checker guarantees argument types
    /// before the interpreter calls a built-in, so a panic here is a bug
    /// in the checker, not in the user's program.
    pub fn as_real(&self) -> f64 {
        match self {
            Value::Real(v) => *v,
            other => panic!("expected real, got {other:?}"),
        }
    }

    /// Text of a string. Panics on other variants, like `as_real`.
    pub fn as_str(&self) -> &str {
        match self {
            Value::Str(text) => text,
            other => panic!("expected a string, got {other:?}"),
        }
    }

    /// Elements of an array. Panics on other variants, like `as_real`.
    pub fn as_array(&self) -> &[Value] {
        match self {
            Value::Array(items) => items,
            other => panic!("expected an array, got {other:?}"),
        }
    }

    /// Components of a vector. Panics on other variants, like `as_real`.
    pub fn components(&self) -> &[f64] {
        match self {
            Value::Vec2(a) => a,
            Value::Vec3(a) => a,
            Value::Vec4(a) => a,
            other => panic!("expected a vector, got {other:?}"),
        }
    }

    /// Applies `f` to every component of a vector. `None` if `self` is
    /// not a vector.
    pub fn map_components(&self, f: impl Fn(f64) -> f64) -> Option<Value> {
        let v = match self {
            Value::Vec2(a) => Value::Vec2(a.map(f)),
            Value::Vec3(a) => Value::Vec3(a.map(f)),
            Value::Vec4(a) => Value::Vec4(a.map(f)),
            _ => return None,
        };
        Some(v)
    }

    /// Applies `f` to matching components of two vectors of the same size.
    /// `None` if they are not vectors of the same size.
    pub fn zip_components(&self, other: &Value, f: impl Fn(f64, f64) -> f64) -> Option<Value> {
        let v = match (self, other) {
            (Value::Vec2(a), Value::Vec2(b)) => Value::Vec2(std::array::from_fn(|i| f(a[i], b[i]))),
            (Value::Vec3(a), Value::Vec3(b)) => Value::Vec3(std::array::from_fn(|i| f(a[i], b[i]))),
            (Value::Vec4(a), Value::Vec4(b)) => Value::Vec4(std::array::from_fn(|i| f(a[i], b[i]))),
            _ => return None,
        };
        Some(v)
    }

    /// `(re, im)` of a `Complex`. Panics on other variants, like `as_real`.
    pub fn as_complex(&self) -> (f64, f64) {
        match self {
            Value::Complex { re, im } => (*re, *im),
            other => panic!("expected complex, got {other:?}"),
        }
    }
}

/// How `print` shows values: `42`, `1.0`, `true`, `2.0+3.0i`, `(1.0, 2.0)`,
/// `[1, 2]`, `(1, 2.0)`.
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            // `{:?}` keeps the `.0` on whole numbers: `1.0`, not `1`.
            Value::Real(v) => write!(f, "{v:?}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Complex { re, im } => {
                let sign = if im.is_sign_negative() { '-' } else { '+' };
                write!(f, "{re:?}{sign}{:?}i", im.abs())
            }
            Value::Str(text) => write!(f, "{text}"),
            Value::Vec2(v) => write_components(f, v),
            Value::Vec3(v) => write_components(f, v),
            Value::Vec4(v) => write_components(f, v),
            Value::Array(items) => {
                let parts: Vec<String> = items.iter().map(Value::to_string).collect();
                write!(f, "[{}]", parts.join(", "))
            }
            Value::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(Value::to_string).collect();
                write!(f, "({})", parts.join(", "))
            }
        }
    }
}

fn write_components(f: &mut std::fmt::Formatter<'_>, v: &[f64]) -> std::fmt::Result {
    let parts: Vec<String> = v.iter().map(|c| format!("{c:?}")).collect();
    write!(f, "({})", parts.join(", "))
}
