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
        (UnOp::Neg, _) => v
            .map_components(|c| -c)
            .unwrap_or_else(|| bug_unary(op, &v)),
        (UnOp::Not, _) => bug_unary(op, &v),
    };
    Ok(result)
}

pub fn binary(op: BinOp, l: Value, r: Value) -> Result<Value, String> {
    use Value::{Array, Bool, Complex, Int, Real, Str};

    let result = match (op, &l, &r) {
        (BinOp::Eq, ..) => Bool(l == r),
        (BinOp::Ne, ..) => Bool(l != r),

        (_, Array(a), Array(b)) => array_op(op, a, b)?,
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

fn array_op(op: BinOp, a: &[Value], b: &[Value]) -> Result<Value, String> {
    match op {
        BinOp::Add | BinOp::Sub => {
            if a.len() != b.len() {
                return Err(format!(
                    "arrays have different lengths: {} and {}",
                    a.len(),
                    b.len()
                ));
            }
            let values = a
                .iter()
                .zip(b)
                .map(|(x, y)| binary(op, x.clone(), y.clone()))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Array(Arc::new(values)))
        }
        BinOp::Mul => matrix_mul(a, b),
        _ => panic!("operator {op:?} on two arrays, the checker should have rejected it"),
    }
}

/// `a * b` for a matrix `a` and a matrix or vector `b`.
///
/// An empty `b` has no first element to tell which of the two it is, so
/// multiplying by it is an error unless `a` is empty too.
fn matrix_mul(a: &[Value], b: &[Value]) -> Result<Value, String> {
    if a.is_empty() {
        return Ok(Value::Array(Arc::new(Vec::new())));
    }
    let (rows, cols) = matrix_rows(a)?;
    let n = rows.len();
    let Some(first) = b.first() else {
        return Err(format!(
            "cannot multiply a {n}x{cols} matrix by an empty array"
        ));
    };

    if !matches!(first, Value::Array(_)) {
        if cols != b.len() {
            return Err(format!(
                "cannot multiply a {n}x{cols} matrix by a vector of length {}",
                b.len()
            ));
        }
        let values = rows
            .iter()
            .map(|row| sum_of_products(row, b.iter()))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Value::Array(Arc::new(values)));
    }

    let (other, other_cols) = matrix_rows(b)?;
    if cols != other.len() {
        return Err(format!(
            "cannot multiply a {n}x{cols} matrix by a {}x{other_cols} matrix",
            other.len()
        ));
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
            return Err(format!(
                "matrix is not rectangular: row {i} has {} elements, row 0 has {cols}",
                row.len()
            ));
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

const OVERFLOW: &str = "integer overflow";

#[inline]
pub fn int_op(op: BinOp, a: i64, b: i64) -> Result<Value, String> {
    let checked = |v: Option<i64>| v.map(Value::Int).ok_or_else(|| OVERFLOW.to_owned());

    match op {
        BinOp::Add => checked(a.checked_add(b)),
        BinOp::Sub => checked(a.checked_sub(b)),
        BinOp::Mul => checked(a.checked_mul(b)),
        BinOp::Div if b == 0 => Err("division by zero".to_owned()),
        // Truncates toward zero; only `i64::MIN / -1` overflows.
        BinOp::Div => checked(a.checked_div(b)),
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
