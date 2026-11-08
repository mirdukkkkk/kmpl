//! A function with `-> type` must end in `return` on every path.
//!
//! A block always returns if one of its statements does. A statement
//! always returns if it is `return`, or an `if` with an `else` where both
//! branches always return. Loops never count, even `while true`, since
//! the checker does not evaluate conditions: write a `return` after them.

use kmpl_ast::{Block, Item, Program, Stmt, StmtKind};

use crate::error::CheckError;

pub fn check_returns(program: &Program) -> Vec<CheckError> {
    let mut errors = Vec::new();
    for item in &program.items {
        if let Item::Function(f) = item
            && f.ret.is_some()
            && !always_returns(&f.body)
        {
            errors.push(CheckError::new(
                format!("`{}` may finish without returning a value", f.name.name),
                f.name.span,
            ));
        }
    }
    errors
}

fn always_returns(block: &Block) -> bool {
    block.iter().any(stmt_always_returns)
}

fn stmt_always_returns(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(_) => true,
        StmtKind::If {
            then,
            otherwise: Some(otherwise),
            ..
        } => always_returns(then) && always_returns(otherwise),
        _ => false,
    }
}
