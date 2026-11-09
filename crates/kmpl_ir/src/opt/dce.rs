//! Removes instructions and block parameters whose values nobody uses.

use std::collections::HashSet;

use kmpl_ast::{BinOp, Ty, Type, UnOp};

use super::remove_block_param;
use crate::{Function, Op, Reg, Terminator};

/// Can `op` be dropped, or run when the program would not have run it?
/// It must have no effect and be unable to stop the program.
pub(super) fn is_pure(op: &Op, f: &Function) -> bool {
    let cannot_fail = |reg: &Reg| {
        matches!(
            f.types[reg.0 as usize],
            Ty::Prim(Type::Real | Type::Complex | Type::Vec2 | Type::Vec3 | Type::Vec4 | Type::Str)
        )
    };

    match op {
        Op::Const(_)
        | Op::GetGlobal(_)
        | Op::MakeArray(_)
        | Op::ArrayLen(_)
        | Op::MakeTuple(_)
        | Op::TupleGet { .. }
        | Op::MakeStruct { .. }
        | Op::FieldGet { .. }
        | Op::Swizzle { .. }
        | Op::RangeContinues { .. }
        | Op::AddOverflows { .. } => true,
        Op::Unary {
            op: UnOp::Not | UnOp::BitNot,
            ..
        } => true,
        Op::Unary { op: UnOp::Neg, a } => cannot_fail(a),
        Op::Binary { op, a, b } => match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                cannot_fail(a) && cannot_fail(b)
            }
            // The shift amount may be out of range.
            BinOp::Shl | BinOp::Shr => false,
            _ => true,
        },
        // May fail or have an effect.
        Op::Call { .. }
        | Op::Builtin { .. }
        | Op::SetGlobal(..)
        | Op::ArrayFill { .. }
        | Op::Index { .. }
        | Op::Slice { .. }
        | Op::PathGet { .. }
        | Op::PathSet { .. }
        | Op::CheckStep(_) => false,
    }
}

/// Returns whether anything changed.
pub fn eliminate(f: &mut Function) -> bool {
    let used = used_registers(f);
    let is_used = |reg: &Reg| used.contains(reg);
    let mut changed = false;

    // Parameters first: the first block's are the function's and stay.
    for index in 1..f.blocks.len() {
        let dead: Vec<usize> = f.blocks[index]
            .params
            .iter()
            .enumerate()
            .filter(|(_, param)| !is_used(param))
            .map(|(position, _)| position)
            .collect();

        for &position in dead.iter().rev() {
            remove_block_param(f, index, position);
            changed = true;
        }
    }

    let pure_ops: Vec<Vec<bool>> = f
        .blocks
        .iter()
        .map(|block| block.insts.iter().map(|i| is_pure(&i.op, f)).collect())
        .collect();
    for (block, pure_in_block) in f.blocks.iter_mut().zip(&pure_ops) {
        let mut position = 0;
        let before = block.insts.len();
        block.insts.retain(|inst| {
            position += 1;
            !pure_in_block[position - 1] || inst.dest.is_some_and(|dest| is_used(&dest))
        });
        changed |= block.insts.len() != before;
    }
    changed
}

/// Registers something real depends on, followed backwards from effects,
/// conditions and returned values. A loop that only feeds itself is dead.
fn used_registers(f: &Function) -> HashSet<Reg> {
    let mut used = HashSet::new();
    loop {
        let before = used.len();
        for block in &f.blocks {
            match &block.term {
                Terminator::Branch { cond, .. } => {
                    used.insert(*cond);
                }
                Terminator::Return(Some(value)) => {
                    used.insert(*value);
                }
                _ => {}
            }

            // A jump's arguments matter only for parameters that are used.
            for target in block.term.targets() {
                let params = &f.blocks[target.block.0 as usize].params;
                for (param, arg) in params.iter().zip(&target.args) {
                    if used.contains(param) {
                        used.insert(*arg);
                    }
                }
            }

            for inst in block.insts.iter().rev() {
                let needed =
                    !is_pure(&inst.op, f) || inst.dest.is_some_and(|dest| used.contains(&dest));
                if needed {
                    inst.op.for_each_use(|reg| {
                        used.insert(reg);
                    });
                }
            }
        }

        if used.len() == before {
            return used;
        }
    }
}
