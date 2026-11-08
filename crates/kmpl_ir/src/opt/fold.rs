//! Constant folding: operations on known values are computed here.
//!
//! An operation that would stop the program with an error (`int` overflow,
//! division by zero) is left alone, so the error still happens when, and
//! only if, the program reaches it. `complex` is left to the run-time code
//! so there is one definition of its arithmetic.

use std::collections::HashMap;

use kmpl_ast::{BinOp, UnOp};

use crate::{Constant, Function, Op, Reg};

/// Returns whether anything changed.
pub fn fold(f: &mut Function) -> bool {
    let mut known: HashMap<Reg, Constant> = HashMap::new();
    let mut changed = false;

    for block in &mut f.blocks {
        for inst in &mut block.insts {
            if let Some(result) = compute(&inst.op, &known) {
                inst.op = Op::Const(result);
                changed = true;
            } else if let Some(rewritten) = simplify(&inst.op, &known) {
                inst.op = rewritten;
                changed = true;
            }
            if let (Some(dest), Op::Const(value)) = (inst.dest, &inst.op) {
                known.insert(dest, value.clone());
            }
        }

        let before = block.insts.len();
        block
            .insts
            .retain(|inst| !matches!(&inst.op, Op::CheckStep(step) if is_nonzero(step, &known)));
        changed |= block.insts.len() != before;
    }
    changed
}

fn is_nonzero(reg: &Reg, known: &HashMap<Reg, Constant>) -> bool {
    matches!(known.get(reg), Some(Constant::Int(v)) if *v != 0)
}

/// The result of `op`, if it is known now and cannot fail.
fn compute(op: &Op, known: &HashMap<Reg, Constant>) -> Option<Constant> {
    match op {
        Op::Unary { op, a } => match (op, known.get(a)?) {
            (UnOp::Not, Constant::Bool(v)) => Some(Constant::Bool(!v)),
            (UnOp::Neg, Constant::Int(v)) => v.checked_neg().map(Constant::Int),
            (UnOp::Neg, Constant::Real(v)) => Some(Constant::Real(-v)),
            _ => None,
        },
        Op::Binary { op, a, b } => match (known.get(a)?, known.get(b)?) {
            (Constant::Int(x), Constant::Int(y)) => int_binary(*op, *x, *y),
            (Constant::Real(x), Constant::Real(y)) => real_binary(*op, *x, *y),
            (Constant::Bool(x), Constant::Bool(y)) => match op {
                BinOp::Eq => Some(Constant::Bool(x == y)),
                BinOp::Ne => Some(Constant::Bool(x != y)),
                _ => None,
            },
            _ => None,
        },
        Op::AddOverflows { a, b } => match (known.get(a)?, known.get(b)?) {
            (Constant::Int(x), Constant::Int(y)) => {
                Some(Constant::Bool(x.checked_add(*y).is_none()))
            }
            _ => None,
        },
        _ => None,
    }
}

fn int_binary(op: BinOp, x: i64, y: i64) -> Option<Constant> {
    let int = |v: Option<i64>| v.map(Constant::Int);
    match op {
        BinOp::Add => int(x.checked_add(y)),
        BinOp::Sub => int(x.checked_sub(y)),
        BinOp::Mul => int(x.checked_mul(y)),
        // `checked_div` is `None` for a zero divisor too.
        BinOp::Div => int(x.checked_div(y)),
        BinOp::Eq => Some(Constant::Bool(x == y)),
        BinOp::Ne => Some(Constant::Bool(x != y)),
        BinOp::Lt => Some(Constant::Bool(x < y)),
        BinOp::Le => Some(Constant::Bool(x <= y)),
        BinOp::Gt => Some(Constant::Bool(x > y)),
        BinOp::Ge => Some(Constant::Bool(x >= y)),
        BinOp::And | BinOp::Or => None,
    }
}

fn real_binary(op: BinOp, x: f64, y: f64) -> Option<Constant> {
    let real = |v: f64| Some(Constant::Real(v));
    let boolean = |v: bool| Some(Constant::Bool(v));
    match op {
        BinOp::Add => real(x + y),
        BinOp::Sub => real(x - y),
        BinOp::Mul => real(x * y),
        BinOp::Div => real(x / y),
        BinOp::Eq => boolean(x == y),
        BinOp::Ne => boolean(x != y),
        BinOp::Lt => boolean(x < y),
        BinOp::Le => boolean(x <= y),
        BinOp::Gt => boolean(x > y),
        BinOp::Ge => boolean(x >= y),
        BinOp::And | BinOp::Or => None,
    }
}

/// A range with a known step is an ordinary comparison.
fn simplify(op: &Op, known: &HashMap<Reg, Constant>) -> Option<Op> {
    let Op::RangeContinues {
        value,
        end,
        step,
        inclusive,
    } = op
    else {
        return None;
    };

    let Some(Constant::Int(step)) = known.get(step) else {
        return None;
    };
    let op = match (*step > 0, *step < 0, inclusive) {
        (true, _, false) => BinOp::Lt,
        (true, _, true) => BinOp::Le,
        (_, true, false) => BinOp::Gt,
        (_, true, true) => BinOp::Ge,
        // A zero step has already stopped the program.
        _ => return None,
    };
    Some(Op::Binary {
        op,
        a: *value,
        b: *end,
    })
}
