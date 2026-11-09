//! What the code generator can already translate.

use kmpl_ast::{Ty, Type};
use kmpl_ir::{Constant, Module, Op};

pub const UNSUPPORTED: &str = "kmpl build: this program is not supported yet";

pub fn is_supported(module: &Module) -> bool {
    let number = |ty: &Ty| matches!(ty, Ty::Prim(Type::Int | Type::Real | Type::Bool));
    module.functions.iter().all(|function| {
        function.types.iter().all(number)
            && function.ret.iter().all(number)
            && function
                .blocks
                .iter()
                .flat_map(|block| &block.insts)
                .all(|inst| op_supported(&inst.op))
    })
}

fn op_supported(op: &Op) -> bool {
    match op {
        Op::Const(constant) => matches!(
            constant,
            Constant::Int(_) | Constant::Real(_) | Constant::Bool(_)
        ),
        Op::Unary { .. }
        | Op::Binary { .. }
        | Op::Call { .. }
        | Op::Builtin { .. }
        | Op::GetGlobal(_)
        | Op::SetGlobal(..)
        | Op::CheckStep(_)
        | Op::RangeContinues { .. }
        | Op::AddOverflows { .. } => true,
        _ => false,
    }
}
