//! Rewrites number literals to the type inference picked for them,
//! so later stages can evaluate literals without knowing about types:
//! `let x: real = 1;` stores `Real(1.0)`, `let z: complex = 2.0;` stores
//! `Complex { re: 2.0, im: 0.0 }`.

use std::collections::HashMap;

use kmpl_ast::{Block, Expr, ExprKind, Item, Program, Span, StmtKind, Type};

pub fn rewrite(program: &mut Program, types: &HashMap<Span, Type>) {
    for item in &mut program.items {
        match item {
            Item::Const(c) => expr(&mut c.value, types),
            Item::Function(f) => block(&mut f.body, types),
        }
    }
}

fn block(block: &mut Block, types: &HashMap<Span, Type>) {
    for stmt in block {
        match &mut stmt.kind {
            StmtKind::Let { init, .. } => {
                if let Some(init) = init {
                    expr(init, types);
                }
            }
            StmtKind::Assign { value, .. } => expr(value, types),
            StmtKind::If {
                cond,
                then,
                otherwise,
            } => {
                expr(cond, types);
                self::block(then, types);
                if let Some(otherwise) = otherwise {
                    self::block(otherwise, types);
                }
            }
            StmtKind::While { cond, body } => {
                expr(cond, types);
                self::block(body, types);
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    expr(value, types);
                }
            }
            StmtKind::Expr(e) => expr(e, types),
        }
    }
}

fn expr(e: &mut Expr, types: &HashMap<Span, Type>) {
    match &mut e.kind {
        ExprKind::Int(v) => match types.get(&e.span) {
            Some(Type::Real) => e.kind = ExprKind::Real(*v as f64),
            Some(Type::Complex) => {
                e.kind = ExprKind::Complex {
                    re: *v as f64,
                    im: 0.0,
                }
            }
            _ => {}
        },
        ExprKind::Real(v) => {
            if types.get(&e.span) == Some(&Type::Complex) {
                e.kind = ExprKind::Complex { re: *v, im: 0.0 };
            }
        }
        ExprKind::Complex { .. } | ExprKind::Bool(_) | ExprKind::Var(_) => {}
        ExprKind::Unary { operand, .. } => expr(operand, types),
        ExprKind::Binary { lhs, rhs, .. } => {
            expr(lhs, types);
            expr(rhs, types);
        }
        ExprKind::Call { args, .. } => {
            for arg in args {
                expr(arg, types);
            }
        }
        ExprKind::Swizzle { base, .. } => expr(base, types),
    }
}
