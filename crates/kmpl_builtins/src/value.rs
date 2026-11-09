use std::sync::Arc;

use kmpl_ast::Type;

/// The shape of a struct, shared by all its values.
#[derive(Debug, PartialEq)]
pub struct StructLayout {
    pub name: String,
    pub fields: Vec<String>,
}

impl StructLayout {
    /// Position of a field the checker has verified exists.
    pub fn field(&self, name: &str) -> usize {
        self.fields
            .iter()
            .position(|f| f == name)
            .expect("checker ensures the field exists")
    }
}

/// `values` is in the order of `layout.fields`.
#[derive(Debug, Clone, PartialEq)]
pub struct StructValue {
    pub layout: Arc<StructLayout>,
    pub values: Vec<Value>,
}

/// A value of an enum. The layout's `fields` are the names of the variants,
/// and `values` is what this variant carries.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    pub layout: Arc<StructLayout>,
    pub variant: usize,
    pub values: Vec<Value>,
}

/// A runtime value. One variant per `Type`, plus arrays, tuples, structs,
/// enums and optionals.
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
    /// `[real]`: the numbers directly, next to each other in memory. Which
    /// form an array has follows from its static type, also when it is empty.
    Reals(Arc<Vec<f64>>),
    /// `[int]`, like `Reals`.
    Ints(Arc<Vec<i64>>),
    /// `bytes`, written through `Arc::make_mut` like an array.
    Bytes(Arc<Vec<u8>>),
    /// Never changed after it is built, so it is simply shared.
    Tuple(Arc<[Value]>),
    /// A value too, written through `Arc::make_mut` like an array.
    Struct(Arc<StructValue>),
    /// Never changed after it is built, so it is simply shared.
    Enum(Arc<EnumValue>),
    /// `some(value)` or `none`, shared like an enum.
    Optional(Option<Arc<Value>>),
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
            Value::Bytes(_) => Type::Bytes,
            Value::Array(_) | Value::Reals(_) | Value::Ints(_) => {
                panic!("an array has no `Type`")
            }
            Value::Tuple(_) => panic!("a tuple has no `Type`"),
            Value::Struct(_) => panic!("a struct has no `Type`"),
            Value::Enum(_) => panic!("an enum has no `Type`"),
            Value::Optional(_) => panic!("an optional has no `Type`"),
        }
    }

    /// The inner `f64` of a `Real`.
    ///
    /// Panics on other variants. The checker guarantees argument types
    /// before the runtime calls a built-in, so a panic here is a bug
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

    /// Numbers of a `[real]`. Panics on other variants, like `as_real`.
    pub fn as_reals(&self) -> &[f64] {
        match self {
            Value::Reals(items) => items,
            other => panic!("expected a [real], got {other:?}"),
        }
    }

    /// Numbers of an `[int]`. Panics on other variants, like `as_real`.
    pub fn as_ints(&self) -> &[i64] {
        match self {
            Value::Ints(items) => items,
            other => panic!("expected an [int], got {other:?}"),
        }
    }

    /// The inner `i64` of an `Int`. Panics on other variants, like `as_real`.
    pub fn as_int(&self) -> i64 {
        match self {
            Value::Int(v) => *v,
            other => panic!("expected an int, got {other:?}"),
        }
    }

    /// The contents of a `bytes`. Panics on other variants, like `as_real`.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Value::Bytes(items) => items,
            other => panic!("expected bytes, got {other:?}"),
        }
    }

    /// The length of an array of any form, or the characters of a string.
    /// Panics on other variants.
    pub fn array_len(&self) -> usize {
        match self {
            Value::Str(text) => text.chars().count(),
            Value::Array(items) => items.len(),
            Value::Reals(items) => items.len(),
            Value::Ints(items) => items.len(),
            Value::Bytes(items) => items.len(),
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
            // The same text as an array of `Value::Real` would give.
            Value::Reals(items) => {
                let parts: Vec<String> = items.iter().map(|v| format!("{v:?}")).collect();
                write!(f, "[{}]", parts.join(", "))
            }
            Value::Ints(items) => {
                let parts: Vec<String> = items.iter().map(i64::to_string).collect();
                write!(f, "[{}]", parts.join(", "))
            }
            Value::Bytes(items) => {
                let parts: Vec<String> = items.iter().map(u8::to_string).collect();
                write!(f, "[{}]", parts.join(", "))
            }
            Value::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(Value::to_string).collect();
                write!(f, "({})", parts.join(", "))
            }
            Value::Enum(e) => {
                let name = e.layout.name.rsplit('.').next().unwrap_or_default();
                let variant = &e.layout.fields[e.variant];
                if e.values.is_empty() {
                    return write!(f, "{name}::{variant}");
                }
                let parts: Vec<String> = e.values.iter().map(Value::to_string).collect();
                write!(f, "{name}::{variant}({})", parts.join(", "))
            }
            Value::Optional(Some(value)) => write!(f, "some({value})"),
            Value::Optional(None) => write!(f, "none"),
            Value::Struct(s) => {
                // In a module the name is `lib.geometry.Point`.
                let name = s.layout.name.rsplit('.').next().unwrap_or_default();
                let parts: Vec<String> = s
                    .layout
                    .fields
                    .iter()
                    .zip(&s.values)
                    .map(|(field, value)| format!("{field}: {value}"))
                    .collect();
                if parts.is_empty() {
                    write!(f, "{name} {{ }}")
                } else {
                    write!(f, "{name} {{ {} }}", parts.join(", "))
                }
            }
        }
    }
}

fn write_components(f: &mut std::fmt::Formatter<'_>, v: &[f64]) -> std::fmt::Result {
    let parts: Vec<String> = v.iter().map(|c| format!("{c:?}")).collect();
    write!(f, "({})", parts.join(", "))
}
