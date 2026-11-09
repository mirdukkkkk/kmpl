//! Operators on values.
//!
//! The checker has already verified operand types, so a type combination
//! not handled here is a checker bug and panics. Errors that depend on
//! the values (integer division by zero, overflow) are returned as
//! messages; the caller attaches the span.
//!
//! `&&` and `||` are not here: they skip the right side, so they are
//! lowered to branches.
//!
//! Arrays: `+`, `-` and unary `-` work element by element, an array with a
//! non-array applies the operator to every element, and `*` on two arrays
//! is the matrix product. Lengths are only known here, so a size mismatch
//! is a run-time error.

use std::sync::Arc;

use crate::Value;
use kmpl_ast::{BinOp, UnOp};

pub fn unary(op: UnOp, v: Value) -> Result<Value, String> {
    let result = match (op, &v) {
        (UnOp::Not, Value::Bool(b)) => Value::Bool(!b),
        (UnOp::BitNot, Value::Int(n)) => Value::Int(!n),
        (UnOp::Neg, Value::Int(n)) => Value::Int(n.checked_neg().ok_or(OVERFLOW)?),
        (UnOp::Neg, Value::Real(x)) => Value::Real(-x),
        // `0 - z`, not `-z`: plain negation turns `+0` into `-0`, so `-(1 + 0i)`
        // would be `-1 - 0i`, and `sqrt` and `log` would land on the other
        // side of their branch cut (`sqrt` giving `-i` instead of `i`).
        (UnOp::Neg, Value::Complex { re, im }) => Value::Complex {
            re: 0.0 - re,
            im: 0.0 - im,
        },
        (UnOp::Neg, Value::Array(items)) => map_array(items, |x| unary(op, x.clone()))?,
        (UnOp::Neg, Value::Reals(items)) => {
            Value::Reals(Arc::new(items.iter().map(|x| -x).collect()))
        }
        (UnOp::Neg, Value::Ints(items)) => {
            let negated: Option<Vec<i64>> = items.iter().map(|x| x.checked_neg()).collect();
            Value::Ints(Arc::new(negated.ok_or(OVERFLOW)?))
        }
        (UnOp::Neg, _) => v
            .map_components(|c| -c)
            .unwrap_or_else(|| bug_unary(op, &v)),
        (UnOp::Not | UnOp::BitNot, _) => bug_unary(op, &v),
    };
    Ok(result)
}

pub fn binary(op: BinOp, l: Value, r: Value) -> Result<Value, String> {
    use Value::{Array, Bool, Complex, Int, Ints, Real, Reals, Str};

    let result = match (op, &l, &r) {
        (BinOp::Eq, ..) => Bool(l == r),
        (BinOp::Ne, ..) => Bool(l != r),

        (_, Reals(_) | Ints(_), _) | (_, _, Reals(_) | Ints(_)) => flat_op(op, &l, &r)?,
        (_, Array(a), Array(b)) => array_op(op, a, &r, b)?,
        (_, Array(a), _) => map_array(a, |x| binary(op, x.clone(), r.clone()))?,
        (_, _, Array(b)) => map_array(b, |x| binary(op, l.clone(), x.clone()))?,

        (BinOp::Add, Str(a), Str(b)) => Str(format!("{a}{b}").into()),
        (_, Int(a), Int(b)) => int_op(op, *a, *b)?,
        (_, Real(a), Real(b)) => real_op(op, *a, *b).unwrap_or_else(|| bug(op, &l, &r)),
        (_, Complex { re: a, im: b }, Complex { re: c, im: d }) => {
            let (re, im) = complex_op(op, (*a, *b), (*c, *d)).unwrap_or_else(|| bug(op, &l, &r));
            Complex { re, im }
        }

        // vector + vector, vector - vector
        (BinOp::Add, ..) => l
            .zip_components(&r, |a, b| a + b)
            .unwrap_or_else(|| bug(op, &l, &r)),
        (BinOp::Sub, ..) => l
            .zip_components(&r, |a, b| a - b)
            .unwrap_or_else(|| bug(op, &l, &r)),

        // scaling
        (BinOp::Mul, v, Real(k)) | (BinOp::Mul, Real(k), v) => v
            .map_components(|c| c * k)
            .unwrap_or_else(|| bug(op, &l, &r)),
        (BinOp::Div, v, Real(k)) => v
            .map_components(|c| c / k)
            .unwrap_or_else(|| bug(op, &l, &r)),

        _ => bug(op, &l, &r),
    };
    Ok(result)
}

fn map_array(
    items: &[Value],
    f: impl FnMut(&Value) -> Result<Value, String>,
) -> Result<Value, String> {
    let values = items.iter().map(f).collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Array(Arc::new(values)))
}

/// One element type of a flat array.
trait Element: Copy {
    fn items(value: &Value) -> &[Self];
    fn wrap(items: Vec<Self>) -> Value;
    fn apply(op: BinOp, a: Self, b: Self) -> Result<Self, String>;
}

impl Element for f64 {
    fn items(value: &Value) -> &[f64] {
        value.as_reals()
    }

    fn wrap(items: Vec<f64>) -> Value {
        Value::Reals(Arc::new(items))
    }

    #[inline]
    fn apply(op: BinOp, a: f64, b: f64) -> Result<f64, String> {
        Ok(match op {
            BinOp::Add => a + b,
            BinOp::Sub => a - b,
            BinOp::Mul => a * b,
            BinOp::Div => a / b,
            other => panic!("operator {other:?} on numbers of an array"),
        })
    }
}

impl Element for i64 {
    fn items(value: &Value) -> &[i64] {
        value.as_ints()
    }

    fn wrap(items: Vec<i64>) -> Value {
        Value::Ints(Arc::new(items))
    }

    #[inline]
    fn apply(op: BinOp, a: i64, b: i64) -> Result<i64, String> {
        Ok(int_op(op, a, b)?.as_int())
    }
}

/// An operator with a flat array on at least one side.
fn flat_op(op: BinOp, l: &Value, r: &Value) -> Result<Value, String> {
    use Value::{Array, Int, Ints, Real, Reals};

    match (l, r) {
        (Reals(_), Reals(_)) => zip_flat::<f64>(op, l, r),
        (Ints(_), Ints(_)) => zip_flat::<i64>(op, l, r),
        (Reals(a), Real(k)) => map_flat(a, |x| f64::apply(op, x, *k)),
        (Real(k), Reals(b)) => map_flat(b, |x| f64::apply(op, *k, x)),
        (Ints(a), Int(k)) => map_flat(a, |x| i64::apply(op, x, *k)),
        (Int(k), Ints(b)) => map_flat(b, |x| i64::apply(op, *k, x)),
        (Array(a), Reals(_) | Ints(_)) if op == BinOp::Mul => matrix_mul(a, r),
        // Another kind of element comes out, such as `[real] * vec2`.
        (Reals(a), _) => map_array(&reals_as_values(a), |x| binary(op, x.clone(), r.clone())),
        (_, Reals(b)) => map_array(&reals_as_values(b), |x| binary(op, l.clone(), x.clone())),
        _ => bug(op, l, r),
    }
}

fn reals_as_values(items: &[f64]) -> Vec<Value> {
    items.iter().map(|x| Value::Real(*x)).collect()
}

fn map_flat<T: Element>(items: &[T], f: impl Fn(T) -> Result<T, String>) -> Result<Value, String> {
    let mapped = items.iter().map(|x| f(*x)).collect::<Result<Vec<_>, _>>()?;
    Ok(T::wrap(mapped))
}

/// `+` and `-` of two arrays of one element type.
fn zip_flat<T: Element>(op: BinOp, l: &Value, r: &Value) -> Result<Value, String> {
    let (a, b) = (T::items(l), T::items(r));
    if !matches!(op, BinOp::Add | BinOp::Sub) {
        panic!("operator {op:?} on two arrays, the checker should have rejected it");
    }
    if a.len() != b.len() {
        return Err(different_lengths(a.len(), b.len()));
    }
    let values = a
        .iter()
        .zip(b)
        .map(|(x, y)| T::apply(op, *x, *y))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(T::wrap(values))
}

fn different_lengths(a: usize, b: usize) -> String {
    format!("arrays have different lengths: {a} and {b}")
}

fn array_op(op: BinOp, a: &[Value], r: &Value, b: &[Value]) -> Result<Value, String> {
    match op {
        BinOp::Add | BinOp::Sub => {
            if a.len() != b.len() {
                return Err(different_lengths(a.len(), b.len()));
            }
            let values = a
                .iter()
                .zip(b)
                .map(|(x, y)| binary(op, x.clone(), y.clone()))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Array(Arc::new(values)))
        }
        BinOp::Mul => matrix_mul(a, r),
        _ => panic!("operator {op:?} on two arrays, the checker should have rejected it"),
    }
}

/// `a * b` for a matrix `a` and a matrix or vector `b`.
///
/// An empty `b` has no first element to tell which of the two it is, so
/// multiplying by it is an error unless `a` is empty too.
fn matrix_mul(a: &[Value], b: &Value) -> Result<Value, String> {
    let Some(first_row) = a.first() else {
        return Ok(match b {
            Value::Reals(_) => Value::Reals(Arc::default()),
            Value::Ints(_) => Value::Ints(Arc::default()),
            _ => Value::Array(Arc::default()),
        });
    };
    match first_row {
        Value::Reals(_) => flat_matrix_mul::<f64>(a, b),
        Value::Ints(_) => flat_matrix_mul::<i64>(a, b),
        _ => boxed_matrix_mul(a, b.as_array()),
    }
}

fn empty_right_side(n: usize, cols: usize) -> String {
    format!("cannot multiply a {n}x{cols} matrix by an empty array")
}

fn wrong_vector(n: usize, cols: usize, len: usize) -> String {
    format!("cannot multiply a {n}x{cols} matrix by a vector of length {len}")
}

fn wrong_matrix(n: usize, cols: usize, rows: usize, other_cols: usize) -> String {
    format!("cannot multiply a {n}x{cols} matrix by a {rows}x{other_cols} matrix")
}

/// Terms are added in the same order as `sum_of_products` adds them, which
/// matters for `real`, but row by row, which is much kinder to the cache.
fn flat_matrix_mul<T: Element>(a: &[Value], b: &Value) -> Result<Value, String> {
    let (rows, cols) = flat_rows::<T>(a)?;
    let n = rows.len();

    let matrix = match b {
        Value::Array(other) => other,
        vector => {
            let vector = T::items(vector);
            if vector.is_empty() {
                return Err(empty_right_side(n, cols));
            }
            if cols != vector.len() {
                return Err(wrong_vector(n, cols, vector.len()));
            }
            let values = rows
                .iter()
                .map(|row| {
                    let mut total = T::apply(BinOp::Mul, row[0], vector[0])?;
                    for (p, q) in row.iter().zip(vector).skip(1) {
                        total = T::apply(BinOp::Add, total, T::apply(BinOp::Mul, *p, *q)?)?;
                    }
                    Ok(total)
                })
                .collect::<Result<Vec<T>, String>>()?;
            return Ok(T::wrap(values));
        }
    };

    if matrix.is_empty() {
        return Err(empty_right_side(n, cols));
    }
    let (other, other_cols) = flat_rows::<T>(matrix)?;
    if cols != other.len() {
        return Err(wrong_matrix(n, cols, other.len(), other_cols));
    }

    let mut result = Vec::with_capacity(n);
    for row in &rows {
        let mut entries = other[0]
            .iter()
            .map(|q| T::apply(BinOp::Mul, row[0], *q))
            .collect::<Result<Vec<T>, String>>()?;
        for (p, other_row) in row.iter().zip(&other).skip(1) {
            for (entry, q) in entries.iter_mut().zip(*other_row) {
                *entry = T::apply(BinOp::Add, *entry, T::apply(BinOp::Mul, *p, *q)?)?;
            }
        }
        result.push(T::wrap(entries));
    }
    Ok(Value::Array(Arc::new(result)))
}

/// The rows of a non-empty matrix of flat arrays and its number of columns.
fn flat_rows<T: Element>(matrix: &[Value]) -> Result<(Vec<&[T]>, usize), String> {
    let rows: Vec<&[T]> = matrix.iter().map(T::items).collect();
    let cols = rows[0].len();
    for (i, row) in rows.iter().enumerate() {
        if row.len() != cols {
            return Err(not_rectangular(i, row.len(), cols));
        }
    }
    Ok((rows, cols))
}

fn not_rectangular(row: usize, len: usize, cols: usize) -> String {
    format!("matrix is not rectangular: row {row} has {len} elements, row 0 has {cols}")
}

/// A matrix of anything else, such as `complex`.
fn boxed_matrix_mul(a: &[Value], b: &[Value]) -> Result<Value, String> {
    let (rows, cols) = matrix_rows(a)?;
    let n = rows.len();
    let Some(first) = b.first() else {
        return Err(empty_right_side(n, cols));
    };

    if !matches!(first, Value::Array(_)) {
        if cols != b.len() {
            return Err(wrong_vector(n, cols, b.len()));
        }
        let values = rows
            .iter()
            .map(|row| sum_of_products(row, b.iter()))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Value::Array(Arc::new(values)));
    }

    let (other, other_cols) = matrix_rows(b)?;
    if cols != other.len() {
        return Err(wrong_matrix(n, cols, other.len(), other_cols));
    }
    let values = rows
        .iter()
        .map(|row| {
            let entries = (0..other_cols)
                .map(|k| sum_of_products(row, other.iter().map(|r| &r[k])))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Array(Arc::new(entries)))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Value::Array(Arc::new(values)))
}

/// The rows of a non-empty matrix and its number of columns.
fn matrix_rows(matrix: &[Value]) -> Result<(Vec<&[Value]>, usize), String> {
    let rows: Vec<&[Value]> = matrix.iter().map(Value::as_array).collect();
    let cols = rows[0].len();
    for (i, row) in rows.iter().enumerate() {
        if row.len() != cols {
            return Err(not_rectangular(i, row.len(), cols));
        }
    }
    Ok((rows, cols))
}

/// `x[0] * y[0] + x[1] * y[1] + ...`. There is at least one term: without
/// knowing the number type there would be no zero to start from.
fn sum_of_products<'v>(x: &[Value], y: impl Iterator<Item = &'v Value>) -> Result<Value, String> {
    let mut total = None;
    for (p, q) in x.iter().zip(y) {
        total = Some(match (total, p, q) {
            // Matrices are nearly always `real`.
            (Some(Value::Real(sum)), Value::Real(a), Value::Real(b)) => Value::Real(sum + a * b),
            (None, Value::Real(a), Value::Real(b)) => Value::Real(a * b),
            (total, p, q) => {
                let term = binary(BinOp::Mul, p.clone(), q.clone())?;
                match total {
                    Some(sum) => binary(BinOp::Add, sum, term)?,
                    None => term,
                }
            }
        });
    }
    Ok(total.expect("the caller checked there is at least one column"))
}

pub const OVERFLOW: &str = "integer overflow";
pub const DIVISION_BY_ZERO: &str = "division by zero";
pub const ZERO_STEP: &str = "loop step cannot be zero";
pub const BAD_SHIFT: &str = "shift amount must be from 0 to 63";

/// `v` as an element of `bytes`.
pub fn byte(v: i64) -> Result<u8, String> {
    u8::try_from(v).map_err(|_| format!("byte value {v} is not from 0 to 255"))
}

/// `n` as the amount of a shift.
pub fn shift_amount(n: i64) -> Result<u32, String> {
    match u32::try_from(n) {
        Ok(amount) if amount < 64 => Ok(amount),
        _ => Err(BAD_SHIFT.to_owned()),
    }
}

/// Deepest allowed chain of function calls. Deeper recursion stops with an
/// error instead of using up all memory.
pub const MAX_CALL_DEPTH: usize = 10_000;

/// `i` as a position in an array of length `len`.
pub fn checked_index(i: i64, len: usize) -> Result<usize, String> {
    match usize::try_from(i) {
        Ok(position) if position < len => Ok(position),
        _ => Err(format!(
            "index {i} is out of bounds for an array of length {len}"
        )),
    }
}

/// The range `from..to` of an array of length `len`, or `from..=to`.
pub fn slice_bounds(
    from: i64,
    to: i64,
    inclusive: bool,
    len: usize,
) -> Result<std::ops::Range<usize>, String> {
    let bounds = (|| {
        let start = usize::try_from(from).ok()?;
        let end = usize::try_from(to).ok()?;
        let end = if inclusive { end.checked_add(1)? } else { end };
        (start <= end && end <= len).then_some(start..end)
    })();
    let dots = if inclusive { "..=" } else { ".." };
    bounds.ok_or_else(|| {
        format!("slice {from}{dots}{to} is out of bounds for an array of length {len}")
    })
}

/// `[item; n]`. The item says which form the array has, because it is known
/// here even when `n` is zero.
pub fn array_fill(item: Value, n: i64) -> Result<Value, String> {
    fn filled<T: Clone>(item: T, n: i64) -> Result<Vec<T>, String> {
        let count =
            usize::try_from(n).map_err(|_| format!("array length cannot be negative, got {n}"))?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| format!("an array of {n} elements is too large"))?;
        values.resize(count, item);
        Ok(values)
    }

    Ok(match item {
        Value::Real(v) => Value::Reals(Arc::new(filled(v, n)?)),
        Value::Int(v) => Value::Ints(Arc::new(filled(v, n)?)),
        other => Value::Array(Arc::new(filled(other, n)?)),
    })
}

pub fn stack_overflow_message() -> String {
    format!("stack overflow: more than {MAX_CALL_DEPTH} nested calls")
}

#[inline]
pub fn int_op(op: BinOp, a: i64, b: i64) -> Result<Value, String> {
    let checked = |v: Option<i64>| v.map(Value::Int).ok_or_else(|| OVERFLOW.to_owned());

    match op {
        BinOp::Add => checked(a.checked_add(b)),
        BinOp::Sub => checked(a.checked_sub(b)),
        BinOp::Mul => checked(a.checked_mul(b)),
        BinOp::Div if b == 0 => Err(DIVISION_BY_ZERO.to_owned()),
        // Truncates toward zero; only `i64::MIN / -1` overflows.
        BinOp::Div => checked(a.checked_div(b)),
        BinOp::Rem if b == 0 => Err(DIVISION_BY_ZERO.to_owned()),
        BinOp::Rem => checked(a.checked_rem(b)),
        BinOp::BitAnd => Ok(Value::Int(a & b)),
        BinOp::BitOr => Ok(Value::Int(a | b)),
        BinOp::BitXor => Ok(Value::Int(a ^ b)),
        BinOp::Shl => Ok(Value::Int(a << shift_amount(b)?)),
        BinOp::Shr => Ok(Value::Int(a >> shift_amount(b)?)),
        BinOp::Lt => Ok(Value::Bool(a < b)),
        BinOp::Le => Ok(Value::Bool(a <= b)),
        BinOp::Gt => Ok(Value::Bool(a > b)),
        BinOp::Ge => Ok(Value::Bool(a >= b)),
        BinOp::Eq | BinOp::Ne | BinOp::And | BinOp::Or => {
            unreachable!("handled elsewhere")
        }
    }
}

/// IEEE 754: dividing by zero gives `inf` or `NaN`, not an error.
#[inline]
pub fn real_op(op: BinOp, a: f64, b: f64) -> Option<Value> {
    let v = match op {
        BinOp::Add => Value::Real(a + b),
        BinOp::Sub => Value::Real(a - b),
        BinOp::Mul => Value::Real(a * b),
        BinOp::Div => Value::Real(a / b),
        BinOp::Rem => Value::Real(a % b),
        BinOp::Lt => Value::Bool(a < b),
        BinOp::Le => Value::Bool(a <= b),
        BinOp::Gt => Value::Bool(a > b),
        BinOp::Ge => Value::Bool(a >= b),
        _ => return None,
    };
    Some(v)
}

/// `(a + bi) op (c + di)` for `+ - * /`.
fn complex_op(op: BinOp, (a, b): (f64, f64), (c, d): (f64, f64)) -> Option<(f64, f64)> {
    let v = match op {
        BinOp::Add => (a + c, b + d),
        BinOp::Sub => (a - c, b - d),
        BinOp::Mul => (a * c - b * d, a * d + b * c),
        BinOp::Div => {
            let den = c * c + d * d;
            ((a * c + b * d) / den, (b * c - a * d) / den)
        }
        _ => return None,
    };
    Some(v)
}

/// `v:zyx`. The checker guarantees the mask uses only valid letters.
pub fn swizzle(v: Value, mask: &str) -> Value {
    let components = v.components();

    let picked: Vec<f64> = mask
        .chars()
        .map(|c| {
            let index = "xyzw".find(c).expect("checked swizzle letter");
            components[index]
        })
        .collect();

    match picked.as_slice() {
        [x] => Value::Real(*x),
        [x, y] => Value::Vec2([*x, *y]),
        [x, y, z] => Value::Vec3([*x, *y, *z]),
        [x, y, z, w] => Value::Vec4([*x, *y, *z, *w]),
        _ => panic!("swizzle `{mask}` has a bad length, the checker should have rejected it"),
    }
}

fn bug(op: BinOp, l: &Value, r: &Value) -> ! {
    panic!("operator {op:?} on {l:?} and {r:?}, the checker should have rejected it")
}

fn bug_unary(op: UnOp, v: &Value) -> ! {
    panic!("operator {op:?} on {v:?}, the checker should have rejected it")
}
