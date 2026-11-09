//! After a successful check, rewrites the program so later stages need no
//! type information:
//! - number literals get the type inference picked for them:
//!   `let x: real = 1;` stores `Real(1.0)`, `let z: complex = 2.0;` stores
//!   `Complex { re: 2.0, im: 0.0 }`;
//! - calls to generic functions are renamed to the copy they use
//!   (`lerp(...)` becomes `lerp<vec2>(...)`).
//!
//! Both are keyed by item index and span, because copies of a generic
//! function share the spans of the original.

use std::collections::HashMap;

use kmpl_ast::{Block, Expr, ExprKind, Item, Program, Span, StmtKind, Type};

use crate::infer::Inference;

struct Rewriter<'i> {
    item: usize,
    literal_types: &'i HashMap<(usize, Span), Type>,
    call_targets: &'i HashMap<(usize, Span), String>,
}

pub fn rewrite(program: &mut Program, inference: &Inference) {
    for (index, item) in program.items.iter_mut().enumerate() {
        let r = Rewriter {
            item: index,
            literal_types: &inference.literal_types,
            call_targets: &inference.call_targets,
        };
        match item {
            Item::Const(c) => r.expr(&mut c.value),
            Item::Function(f) => r.block(&mut f.body),
            Item::Struct(_) | Item::Enum(_) => {}
        }
    }
}

impl Rewriter<'_> {
    fn block(&self, block: &mut Block) {
        for stmt in block {
            match &mut stmt.kind {
                StmtKind::Let { init, .. } => {
                    if let Some(init) = init {
                        self.expr(init);
                    }
                }
                StmtKind::LetTuple { value, .. } | StmtKind::Assign { value, .. } => {
                    self.expr(value)
                }
                StmtKind::PathAssign { path, value, .. } => {
                    for step in path {
                        for index in step.indices_mut() {
                            self.expr(index);
                        }
                    }
                    self.expr(value);
                }
                StmtKind::ForEach { array, body, .. } => {
                    self.expr(array);
                    self.block(body);
                }
                StmtKind::If {
                    cond,
                    then,
                    otherwise,
                } => {
                    self.expr(cond);
                    self.block(then);
                    if let Some(otherwise) = otherwise {
                        self.block(otherwise);
                    }
                }
                StmtKind::While { cond, body } => {
                    self.expr(cond);
                    self.block(body);
                }
                StmtKind::For {
                    start,
                    end,
                    step,
                    body,
                    ..
                } => {
                    self.expr(start);
                    self.expr(end);
                    if let Some(step) = step {
                        self.expr(step);
                    }
                    self.block(body);
                }
                StmtKind::Match { value, arms } => {
                    self.expr(value);
                    for arm in arms {
                        self.block(&mut arm.body);
                    }
                }
                StmtKind::Break | StmtKind::Continue => {}
                StmtKind::Return(value) => {
                    if let Some(value) = value {
                        self.expr(value);
                    }
                }
                StmtKind::Expr(e) => self.expr(e),
            }
        }
    }

    fn expr(&self, e: &mut Expr) {
        let key = (self.item, e.span);
        match &mut e.kind {
            ExprKind::Int(v) => match self.literal_types.get(&key) {
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
                if self.literal_types.get(&key) == Some(&Type::Complex) {
                    e.kind = ExprKind::Complex { re: *v, im: 0.0 };
                }
            }
            ExprKind::Complex { .. }
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Var(_)
            | ExprKind::None => {}
            ExprKind::Variant { args, .. } => {
                for arg in args {
                    self.expr(arg);
                }
            }
            ExprKind::Some(value) => self.expr(value),
            ExprKind::Lambda(l) => self.block(&mut l.body),
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Call { name, args } => {
                if let Some(target) = self.call_targets.get(&key) {
                    name.name = target.clone();
                }
                for arg in args {
                    self.expr(arg);
                }
            }
            ExprKind::Swizzle { base, .. } | ExprKind::Field { base, .. } => self.expr(base),
            ExprKind::StructLit { fields, .. } => {
                for (_, value) in fields {
                    self.expr(value);
                }
            }
            ExprKind::Array(items) | ExprKind::Tuple(items) => {
                for item in items {
                    self.expr(item);
                }
            }
            ExprKind::ArrayFill { value, len } => {
                self.expr(value);
                self.expr(len);
            }
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
            }
            ExprKind::Cell { base, row, col } => {
                self.expr(base);
                self.expr(row);
                self.expr(col);
            }
            ExprKind::Slice {
                base, start, end, ..
            } => {
                self.expr(base);
                for bound in [start, end].into_iter().flatten() {
                    self.expr(bound);
                }
            }
        }
    }
}
