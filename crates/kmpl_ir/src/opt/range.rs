//! Removes the overflow check of `for` loops that cannot overflow.
//!
//! The counter of `a..b` with step 1 has passed `counter < b` when it is
//! stepped, so `counter + 1` is at most `b` and fits an `int`. The same
//! holds for step -1. `..=` can reach the last `int`, and a larger step
//! can jump past it, so those keep the check.
//!
//! Must run before folding turns range tests into comparisons.

use std::collections::{HashMap, HashSet};

use kmpl_ast::UnOp;

use crate::{Constant, Function, Op, Reg};

pub fn simplify(f: &mut Function) {
    let mut known: HashMap<Reg, i64> = HashMap::new();
    let mut safe: HashSet<(Reg, Reg)> = HashSet::new();

    for inst in f.blocks.iter().flat_map(|block| &block.insts) {
        match (&inst.op, inst.dest) {
            (Op::Const(Constant::Int(value)), Some(dest)) => {
                known.insert(dest, *value);
            }
            // `step -1` is a negated literal at this point.
            (Op::Unary { op: UnOp::Neg, a }, Some(dest)) => {
                if let Some(value) = known.get(a).and_then(|v| v.checked_neg()) {
                    known.insert(dest, value);
                }
            }
            (
                Op::RangeContinues {
                    value,
                    step,
                    inclusive: false,
                    ..
                },
                _,
            ) if matches!(known.get(step), Some(1 | -1)) => {
                safe.insert((*value, *step));
            }
            _ => {}
        }
    }

    for inst in f.blocks.iter_mut().flat_map(|block| &mut block.insts) {
        if let Op::AddOverflows { a, b } = inst.op
            && safe.contains(&(a, b))
        {
            inst.op = Op::Const(Constant::Bool(false));
        }
    }
}
