//! Built-in functions, in one table.
//!
//! The checker reads `generic`, `params` and `ret` from here, the
//! runtime calls `func`. Adding a built-in means adding one entry
//! to `BUILTINS`, or to a file in `stdlib/` for a `std:` library.

pub mod abi;
mod complex;
mod math;
mod matrix;
pub mod ops;
mod stdlib;
mod value;

#[cfg(test)]
mod tests;

pub use matrix::Matrix;
pub use stdlib::Random;
pub use value::{EnumValue, StructLayout, StructValue, Value};

use std::io::{BufRead, Write};
use std::sync::Arc;
use std::time::Instant;

use kmpl_ast::Type;

pub struct Builtin {
    pub name: &'static str,
    /// Types the generic parameter `T` may stand for. Empty if the
    /// built-in does not use `T`. All `T` positions in one call are the
    /// same type: `sqrt(T) -> T` with `T` in `[real, complex]`.
    pub generic: &'static [Type],
    pub params: &'static [ParamType],
    pub ret: RetType,
    /// Called with arguments whose types already match `params`. Returns
    /// the value (`None` for `RetType::Nothing`) or a runtime error message.
    pub func: BuiltinFn,
}

/// The arguments are mutable so a built-in can reuse one it gets to own,
/// instead of copying it.
pub type BuiltinFn = fn(&mut [Value], &mut Io) -> Result<Option<Value>, String>;

/// What a running program can reach outside itself.
pub struct Io<'a> {
    /// Where `print` writes.
    pub out: &'a mut dyn Write,
    /// Where `read_line` reads from.
    pub input: &'a mut dyn BufRead,
    /// What `args` returns.
    pub args: &'a [String],
    /// The generator of `std:random`.
    pub random: &'a mut Random,
    /// When the program started, for `std:time`.
    pub started: Instant,
}

/// Built-ins whose value depends on how the program is run, so constants
/// cannot use them. Those that return nothing are excluded anyway.
const DEPENDS_ON_RUN: &[&str] = &[
    "_io_args",
    "_io_read_line",
    "_fs_read",
    "_fs_read_text",
    "_fs_write",
    "_random_real",
    "_random_int",
    "_random_normal",
    "_random_shuffle",
    "_time_now",
    "_time_clock",
];

pub fn depends_on_run(name: &str) -> bool {
    DEPENDS_ON_RUN.contains(&name)
}

/// What a built-in accepts in one argument position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    Exact(Type),
    /// The generic type `T`, see `Builtin::generic`.
    T,
    /// Any type. Used by `print`.
    Any,
    /// An array of any element type, a string, `bytes` or a vector. Used
    /// by `length`.
    Collection,
    /// The element type `E`: any type, arrays included. All `E` positions
    /// in one call are the same type. Used by `push`.
    Elem,
    /// An array of `E`.
    ArrayOfElem,
    /// `E?`
    OptionalOfElem,
    /// `[[real]]`
    Rows,
    /// `[real]`
    Reals,
}

/// What a built-in returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetType {
    Nothing,
    Exact(Type),
    /// The generic type `T`, see `Builtin::generic`.
    T,
    /// Like `T`, but `complex` gives `real`: `abs` of `int` is `int`,
    /// of `real` is `real`, of `complex` is its modulus, a `real`.
    Magnitude,
    /// An array of `E`, see `ParamType::Elem`.
    ArrayOfElem,
    /// `E` itself.
    Elem,
    /// An array of exactly this element type.
    ArrayOf(Type),
    /// A tuple of two values of these types.
    Pair(Type, Type),
    /// `T?` of this type.
    Optional(Type),
    /// `real` for a vector, `int` for what is counted. Used by `length`.
    Length,
    /// `[[real]]`
    Rows,
    /// `[real]?`
    OptionalReals,
}

const REAL: ParamType = ParamType::Exact(Type::Real);
const COMPLEX: ParamType = ParamType::Exact(Type::Complex);
const REAL_OR_COMPLEX: &[Type] = &[Type::Real, Type::Complex];
const INT_OR_REAL: &[Type] = &[Type::Int, Type::Real];
const NUMBER: &[Type] = &[Type::Int, Type::Real, Type::Complex];
const VECTOR: &[Type] = &[Type::Vec2, Type::Vec3, Type::Vec4];
const REAL_OR_VECTOR: &[Type] = &[Type::Real, Type::Vec2, Type::Vec3, Type::Vec4];
const FLOAT_OR_VECTOR: &[Type] = &[
    Type::Real,
    Type::Complex,
    Type::Vec2,
    Type::Vec3,
    Type::Vec4,
];
const VEC3: ParamType = ParamType::Exact(Type::Vec3);

static BUILTINS: &[Builtin] = &[
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
        generic: NUMBER,
        params: &[ParamType::T],
        ret: RetType::Magnitude,
        func: |a, _| match a[0].clone() {
            Value::Int(n) => math::int_abs(n).map(|n| Some(Value::Int(n))),
            Value::Complex { re, im } => ok(Value::Real(re.hypot(im))),
            other => ok(Value::Real(other.as_real().abs())),
        },
    },
    Builtin {
        name: "pow",
        generic: NUMBER,
        params: &[ParamType::T, ParamType::T],
        ret: RetType::T,
        func: |a, _| match (a[0].clone(), a[1].clone()) {
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
    Builtin {
        name: "min",
        generic: INT_OR_REAL,
        params: &[ParamType::T, ParamType::T],
        ret: RetType::T,
        func: |a, _| match (a[0].clone(), a[1].clone()) {
            (Value::Int(x), Value::Int(y)) => ok(Value::Int(x.min(y))),
            // `f64::min` ignores a NaN argument and returns the other one.
            (x, y) => ok(Value::Real(x.as_real().min(y.as_real()))),
        },
    },
    Builtin {
        name: "max",
        generic: INT_OR_REAL,
        params: &[ParamType::T, ParamType::T],
        ret: RetType::T,
        func: |a, _| match (a[0].clone(), a[1].clone()) {
            (Value::Int(x), Value::Int(y)) => ok(Value::Int(x.max(y))),
            (x, y) => ok(Value::Real(x.as_real().max(y.as_real()))),
        },
    },
    Builtin {
        name: "clamp",
        generic: INT_OR_REAL,
        params: &[ParamType::T, ParamType::T, ParamType::T],
        ret: RetType::T,
        func: |a, _| math::clamp(a[0].clone(), a[1].clone(), a[2].clone()).map(Some),
    },
    Builtin {
        name: "lerp",
        generic: FLOAT_OR_VECTOR,
        params: &[ParamType::T, ParamType::T, REAL],
        ret: RetType::T,
        func: |a, _| ok(math::lerp(a[0].clone(), a[1].clone(), a[2].as_real())),
    },
    Builtin {
        name: "smoothstep",
        generic: &[],
        params: &[REAL, REAL, REAL],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            let (e0, e1, x) = (a[0].as_real(), a[1].as_real(), a[2].as_real());
            ok(Value::Real(math::smoothstep(e0, e1, x)))
        },
    },
    // ----- conversions -----
    Builtin {
        name: "real",
        generic: INT_OR_REAL,
        params: &[ParamType::T],
        ret: RetType::Exact(Type::Real),
        func: |a, _| match a[0].clone() {
            Value::Int(n) => ok(Value::Real(n as f64)),
            other => ok(Value::Real(other.as_real())),
        },
    },
    Builtin {
        name: "int",
        generic: INT_OR_REAL,
        params: &[ParamType::T],
        ret: RetType::Exact(Type::Int),
        func: |a, _| match a[0].clone() {
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
        name: "arg",
        generic: &[],
        params: &[COMPLEX],
        ret: RetType::Exact(Type::Real),
        func: |a, _| {
            let (re, im) = a[0].as_complex();
            ok(Value::Real(im.atan2(re)))
        },
    },
    Builtin {
        name: "conj",
        generic: &[],
        params: &[COMPLEX],
        ret: RetType::Exact(Type::Complex),
        func: |a, _| {
            let (re, im) = a[0].as_complex();
            ok(Value::Complex { re, im: 0.0 - im })
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
    Builtin {
        name: "dot",
        generic: VECTOR,
        params: &[ParamType::T, ParamType::T],
        ret: RetType::Exact(Type::Real),
        func: |a, _| ok(Value::Real(math::dot(&a[0], &a[1]))),
    },
    Builtin {
        name: "distance",
        generic: REAL_OR_VECTOR,
        params: &[ParamType::T, ParamType::T],
        ret: RetType::Exact(Type::Real),
        func: |a, _| match (a[0].clone(), a[1].clone()) {
            (Value::Real(x), Value::Real(y)) => ok(Value::Real((x - y).abs())),
            (u, v) => {
                let d = u
                    .zip_components(&v, |x, y| x - y)
                    .expect("same vector type");
                ok(Value::Real(math::dot(&d, &d).sqrt()))
            }
        },
    },
    Builtin {
        name: "normalize",
        generic: VECTOR,
        params: &[ParamType::T],
        ret: RetType::T,
        // A zero vector gives NaN components (0 / 0), as in GLSL.
        func: |a, _| {
            let len = math::dot(&a[0], &a[0]).sqrt();
            ok(a[0].map_components(|c| c / len).expect("a vector"))
        },
    },
    Builtin {
        name: "cross",
        generic: &[],
        params: &[VEC3, VEC3],
        ret: RetType::Exact(Type::Vec3),
        func: |a, _| {
            let (u, v) = (a[0].components(), a[1].components());
            ok(Value::Vec3([
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ]))
        },
    },
    // ----- arrays -----
    Builtin {
        name: "length",
        generic: &[],
        params: &[ParamType::Collection],
        ret: RetType::Length,
        func: |a, _| match &a[0] {
            // Characters, not bytes: `length("привет")` is 6.
            Value::Str(text) => ok(Value::Int(text.chars().count() as i64)),
            vector @ (Value::Vec2(_) | Value::Vec3(_) | Value::Vec4(_)) => {
                ok(Value::Real(math::dot(vector, vector).sqrt()))
            }
            array => ok(Value::Int(array.array_len() as i64)),
        },
    },
    Builtin {
        name: "unwrap",
        generic: &[],
        params: &[ParamType::OptionalOfElem],
        ret: RetType::Elem,
        func: |a, _| match take_optional(&mut a[0]) {
            Some(value) => ok(value),
            None => Err("`unwrap` found `none`".to_owned()),
        },
    },
    Builtin {
        name: "unwrap_or",
        generic: &[],
        params: &[ParamType::OptionalOfElem, ParamType::Elem],
        ret: RetType::Elem,
        func: |a, _| {
            let value = take_optional(&mut a[0]);
            ok(value.unwrap_or_else(|| std::mem::replace(&mut a[1], Value::Int(0))))
        },
    },
    Builtin {
        name: "format",
        generic: &[
            Type::Int,
            Type::Real,
            Type::Complex,
            Type::Vec2,
            Type::Vec3,
            Type::Vec4,
        ],
        params: &[ParamType::T, ParamType::Exact(Type::Int)],
        ret: RetType::Exact(Type::Str),
        func: |a, _| {
            let digits = a[1].as_int();
            if !(0..=MAX_FORMAT_DIGITS).contains(&digits) {
                return Err(format!(
                    "`format` takes from 0 to {MAX_FORMAT_DIGITS} digits, got {digits}"
                ));
            }
            ok(Value::Str(formatted(&a[0], digits as usize).into()))
        },
    },
    // Arrays are values, so these return a new array and leave the
    // argument alone: `a = push(a, x);`. If nothing else holds the array it
    // grows in place, so a loop of pushes does not copy it each time.
    Builtin {
        name: "push",
        generic: &[],
        params: &[ParamType::ArrayOfElem, ParamType::Elem],
        ret: RetType::ArrayOfElem,
        func: |a, _| {
            let mut array = std::mem::replace(&mut a[0], Value::Int(0));
            match &mut array {
                Value::Array(items) => Arc::make_mut(items).push(a[1].clone()),
                Value::Reals(items) => Arc::make_mut(items).push(a[1].as_real()),
                Value::Ints(items) => Arc::make_mut(items).push(a[1].as_int()),
                Value::Bytes(items) => Arc::make_mut(items).push(ops::byte(a[1].as_int())?),
                other => panic!("expected an array, got {other:?}"),
            }
            ok(array)
        },
    },
    Builtin {
        name: "concat",
        generic: &[],
        params: &[ParamType::ArrayOfElem, ParamType::ArrayOfElem],
        ret: RetType::ArrayOfElem,
        func: |a, _| {
            let mut array = std::mem::replace(&mut a[0], Value::Int(0));
            match &mut array {
                Value::Array(items) => Arc::make_mut(items).extend_from_slice(a[1].as_array()),
                Value::Reals(items) => Arc::make_mut(items).extend_from_slice(a[1].as_reals()),
                Value::Ints(items) => Arc::make_mut(items).extend_from_slice(a[1].as_ints()),
                Value::Bytes(items) => Arc::make_mut(items).extend_from_slice(a[1].as_bytes()),
                other => panic!("expected an array, got {other:?}"),
            }
            ok(array)
        },
    },
    // ----- matrices -----
    Builtin {
        name: "matrix",
        generic: &[],
        params: &[ParamType::Exact(Type::Int), ParamType::Exact(Type::Int)],
        ret: RetType::Exact(Type::Matrix),
        func: |a, _| {
            let zeros = Matrix::zeros(a[0].as_int(), a[1].as_int())?;
            ok(Value::Matrix(Arc::new(zeros)))
        },
    },
    Builtin {
        name: "rows",
        generic: &[],
        params: &[ParamType::Exact(Type::Matrix)],
        ret: RetType::Exact(Type::Int),
        func: |a, _| ok(Value::Int(a[0].as_matrix().rows as i64)),
    },
    Builtin {
        name: "cols",
        generic: &[],
        params: &[ParamType::Exact(Type::Matrix)],
        ret: RetType::Exact(Type::Int),
        func: |a, _| ok(Value::Int(a[0].as_matrix().cols as i64)),
    },
    Builtin {
        name: "to_matrix",
        generic: &[],
        params: &[ParamType::Rows],
        ret: RetType::Exact(Type::Matrix),
        func: |a, _| ok(Value::Matrix(Arc::new(Matrix::from_rows(&a[0])?))),
    },
    Builtin {
        name: "to_rows",
        generic: &[],
        params: &[ParamType::Exact(Type::Matrix)],
        ret: RetType::Rows,
        func: |a, _| ok(a[0].as_matrix().to_rows()),
    },
    // ----- strings -----
    // Each character as its own string, so that a program can jump around
    // in a text without walking it from the start every time.
    Builtin {
        name: "chars",
        generic: &[],
        params: &[ParamType::Exact(Type::Str)],
        ret: RetType::ArrayOf(Type::Str),
        func: |a, _| {
            let items = a[0]
                .as_str()
                .chars()
                .map(|c| Value::Str(c.to_string().into()))
                .collect();
            ok(Value::Array(Arc::new(items)))
        },
    },
    Builtin {
        name: "string",
        generic: &[],
        params: &[ParamType::Any],
        ret: RetType::Exact(Type::Str),
        func: |a, _| match &a[0] {
            // The text inside the bytes, not the numbers `print` shows.
            Value::Bytes(items) => match std::str::from_utf8(items) {
                Ok(text) => ok(Value::Str(text.into())),
                Err(_) => Err("bytes are not valid UTF-8".to_owned()),
            },
            other => ok(Value::Str(other.to_string().into())),
        },
    },
    Builtin {
        name: "bytes",
        generic: &[],
        params: &[ParamType::Exact(Type::Str)],
        ret: RetType::Exact(Type::Bytes),
        func: |a, _| ok(Value::Bytes(Arc::new(a[0].as_str().as_bytes().to_vec()))),
    },
    // ----- input -----
    // ----- output -----
    Builtin {
        name: "print",
        generic: &[],
        params: &[ParamType::Any],
        ret: RetType::Nothing,
        func: |a, io| {
            writeln!(io.out, "{}", a[0]).map_err(|e| format!("cannot write output: {e}"))?;
            Ok(None)
        },
    },
    Builtin {
        name: "eprint",
        generic: &[],
        params: &[ParamType::Any],
        ret: RetType::Nothing,
        func: |a, io| {
            // So that the two streams appear in the order they were written.
            io.out
                .flush()
                .map_err(|e| format!("cannot write output: {e}"))?;
            eprintln!("{}", a[0]);
            Ok(None)
        },
    },
    // The checker and the lowering know that these two never come back.
    Builtin {
        name: "panic",
        generic: &[],
        params: &[ParamType::Exact(Type::Str)],
        ret: RetType::Nothing,
        func: |a, _| Err(a[0].as_str().to_owned()),
    },
    Builtin {
        name: "exit",
        generic: &[],
        params: &[ParamType::Exact(Type::Int)],
        ret: RetType::Nothing,
        func: |a, io| {
            let code = a[0].as_int();
            if !(0..=255).contains(&code) {
                return Err(format!("`exit` takes a code from 0 to 255, got {code}"));
            }
            io.out
                .flush()
                .map_err(|e| format!("cannot write output: {e}"))?;
            std::process::exit(code as i32);
        },
    },
];

/// Every built-in in a fixed order, which generated code and the runtime
/// use to refer to one by its position.
pub fn all() -> impl Iterator<Item = &'static Builtin> {
    let libraries = stdlib::GROUPS.iter().flat_map(|group| group.iter());
    BUILTINS.iter().chain(libraries)
}

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    all().find(|b| b.name == name)
}

/// A named value every program can use, like a `const` it did not write.
pub struct BuiltinConst {
    pub name: &'static str,
    pub value: Value,
}

pub static CONSTANTS: &[BuiltinConst] = &[
    BuiltinConst {
        name: "PI",
        value: Value::Real(std::f64::consts::PI),
    },
    BuiltinConst {
        name: "TAU",
        value: Value::Real(std::f64::consts::TAU),
    },
    BuiltinConst {
        name: "E",
        value: Value::Real(std::f64::consts::E),
    },
];

pub fn lookup_const(name: &str) -> Option<&'static BuiltinConst> {
    CONSTANTS.iter().find(|c| c.name == name)
}

/// `Ok(Some(value))`, the result of a built-in that cannot fail.
pub(crate) fn ok(value: Value) -> Result<Option<Value>, String> {
    Ok(Some(value))
}

/// What an optional argument holds, moved out if nothing else shares it.
fn take_optional(argument: &mut Value) -> Option<Value> {
    match std::mem::replace(argument, Value::Int(0)) {
        Value::Optional(inner) => inner.map(Arc::unwrap_or_clone),
        other => panic!("expected an optional, got {other:?}"),
    }
}

/// `some(value)` or `none`.
pub(crate) fn optional(value: Option<Value>) -> Value {
    Value::Optional(value.map(Arc::new))
}

const MAX_FORMAT_DIGITS: i64 = 100;

/// The text of a number or vector with this many digits after the point.
fn formatted(value: &Value, digits: usize) -> String {
    let real = |x: f64| format!("{x:.digits$}");
    match value {
        // Written out, because a large `int` does not fit a `real` exactly.
        Value::Int(v) if digits == 0 => v.to_string(),
        Value::Int(v) => format!("{v}.{}", "0".repeat(digits)),
        Value::Real(v) => real(*v),
        Value::Complex { re, im } => {
            let sign = if im.is_sign_negative() { '-' } else { '+' };
            format!("{}{sign}{}i", real(*re), real(im.abs()))
        }
        vector => {
            let parts: Vec<String> = vector.components().iter().map(|c| real(*c)).collect();
            format!("({})", parts.join(", "))
        }
    }
}

pub(crate) fn pair(a: Value, b: Value) -> Value {
    Value::Tuple(vec![a, b].into())
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
    match args[0].clone() {
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
