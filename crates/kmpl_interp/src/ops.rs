//! Operators on values.
//!
//! The checker has already verified operand types, so a type combination
//! not handled here is a checker bug and panics. Errors that depend on
//! the values (integer division by zero, overflow) are returned as
//! messages; `eval` attaches the span.
//!
//! `&&` and `||` are not here: they skip the right side, so `eval`
//! handles them.

use kmpl_ast::{BinOp, UnOp};
use kmpl_builtins::Value;

pub fn unary(op: UnOp, v: Value) -> Result<Value, String> {
    let result = match (op, v) {
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
        (UnOp::Neg, v) => v.map_components(|c| -c).unwrap_or_else(|| bug_unary(op, v)),
        (UnOp::Not, v) => bug_unary(op, v),
    };
    Ok(result)
}

pub fn binary(op: BinOp, l: Value, r: Value) -> Result<Value, String> {
    use Value::{Bool, Complex, Int, Real};

    let result = match (op, l, r) {
        (BinOp::Eq, l, r) => Bool(l == r),
        (BinOp::Ne, l, r) => Bool(l != r),

        (_, Int(a), Int(b)) => int_op(op, a, b)?,
        (_, Real(a), Real(b)) => real_op(op, a, b).unwrap_or_else(|| bug(op, l, r)),
        (_, Complex { re: a, im: b }, Complex { re: c, im: d }) => {
            let (re, im) = complex_op(op, (a, b), (c, d)).unwrap_or_else(|| bug(op, l, r));
            Complex { re, im }
        }

        // vector + vector, vector - vector
        (BinOp::Add, l, r) => l
            .zip_components(r, |a, b| a + b)
            .unwrap_or_else(|| bug(op, l, r)),
        (BinOp::Sub, l, r) => l
            .zip_components(r, |a, b| a - b)
            .unwrap_or_else(|| bug(op, l, r)),

        // scaling
        (BinOp::Mul, v, Real(k)) | (BinOp::Mul, Real(k), v) => {
            v.map_components(|c| c * k).unwrap_or_else(|| bug(op, l, r))
        }
        (BinOp::Div, v, Real(k)) => v.map_components(|c| c / k).unwrap_or_else(|| bug(op, l, r)),

        _ => bug(op, l, r),
    };
    Ok(result)
}

const OVERFLOW: &str = "integer overflow";

fn int_op(op: BinOp, a: i64, b: i64) -> Result<Value, String> {
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
fn real_op(op: BinOp, a: f64, b: f64) -> Option<Value> {
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

fn bug(op: BinOp, l: Value, r: Value) -> ! {
    panic!(
        "operator {op:?} on {:?} and {:?}, the checker should have rejected it",
        l.ty(),
        r.ty()
    )
}

fn bug_unary(op: UnOp, v: Value) -> ! {
    panic!(
        "operator {op:?} on {:?}, the checker should have rejected it",
        v.ty()
    )
}
