//! Definite assignment: a variable declared with `let x;` may only be read
//! after it has been assigned on every path that reaches the read.
//!
//! Rules:
//! - `x = ...` initializes `x`. `x += ...`, `x[i] = ...` and `x.f = ...` read `x`, so `x`
//!   must already be assigned.
//! - After `if`/`else`, `x` is initialized only if both branches initialize it.
//!   An `if` without `else` initializes nothing.
//! - Assignments inside `while` and `for` do not count after the loop: the
//!   body may run zero times.
//! - A branch that ends in `return`, `break` or `continue` never reaches the
//!   code after it, so it does not block initialization.
//! - A bad read marks the variable as assigned from then on, so one
//!   missing assignment does not produce a chain of errors on that path.
//!
//! Names that are not locals (constants, functions, undefined names) are
//! skipped here; `resolve` reports problems with them.

use std::collections::HashMap;

use kmpl_ast::{Block, Expr, ExprKind, Function, Ident, Item, MatchArm, Program, Stmt, StmtKind};

use crate::error::CheckError;

struct InitChecker<'a> {
    /// Name -> variable id. Innermost scope is last.
    scopes: Vec<HashMap<&'a str, usize>>,
    /// `init[id]`: is variable `id` definitely assigned at this point?
    /// Ids are handed out in declaration order and dropped when their
    /// block ends, so this works like a stack.
    init: Vec<bool>,
    errors: Vec<CheckError>,
}

pub fn check_init(program: &Program) -> Vec<CheckError> {
    let mut c = InitChecker {
        scopes: Vec::new(),
        init: Vec::new(),
        errors: Vec::new(),
    };

    for item in &program.items {
        if let Item::Function(f) = item {
            c.function(f);
        }
    }

    c.errors
}

impl<'a> InitChecker<'a> {
    fn function(&mut self, f: &'a Function) {
        self.scopes.push(HashMap::new());
        for param in &f.params {
            self.declare(&param.name, true);
        }
        self.block(&f.body);
        self.scopes.pop();
        self.init.clear();
    }

    fn declare(&mut self, name: &'a Ident, initialized: bool) {
        let id = self.init.len();
        self.init.push(initialized);
        self.scopes
            .last_mut()
            .expect("inside a function")
            .insert(&name.name, id);
    }

    fn lookup(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    /// Returns `true` if the block always ends in `return`, `break` or
    /// `continue`, so the code after it is never reached from here.
    fn block(&mut self, block: &'a Block) -> bool {
        let mark = self.init.len();
        self.scopes.push(HashMap::new());

        let mut diverges = false;
        for stmt in block {
            if self.stmt(stmt) && !diverges {
                diverges = true;
                // The rest of the block is unreachable: treat everything
                // as assigned there so it reports nothing.
                self.init.fill(true);
            }
        }

        self.scopes.pop();
        self.init.truncate(mark);
        diverges
    }

    /// Returns `true` if the statement never lets control reach the next one.
    fn stmt(&mut self, stmt: &'a Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Let { name, init, .. } => {
                if let Some(init) = init {
                    self.expr(init);
                }
                self.declare(name, init.is_some());
                false
            }
            StmtKind::LetTuple { names, value } => {
                self.expr(value);
                for name in names.iter().filter(|n| n.name != "_") {
                    self.declare(name, true);
                }
                false
            }
            StmtKind::Assign {
                name, op, value, ..
            } => {
                self.expr(value);
                if op.is_some() {
                    self.read(name);
                }
                if let Some(id) = self.lookup(&name.name) {
                    self.init[id] = true;
                }
                false
            }
            StmtKind::PathAssign {
                name, path, value, ..
            } => {
                self.expr(value);
                for step in path {
                    for index in step.indices() {
                        self.expr(index);
                    }
                }
                self.read(name);
                false
            }
            StmtKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.expr(cond);
                self.if_branches(then, otherwise.as_ref())
            }
            StmtKind::While { cond, body } => {
                self.expr(cond);
                let before = self.init.clone();
                self.block(body);
                self.init = before;
                false
            }
            StmtKind::For {
                var,
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
                self.for_body(var, body);
                false
            }
            StmtKind::ForEach { var, array, body } => {
                self.expr(array);
                self.for_body(var, body);
                false
            }
            StmtKind::Match { value, arms } => {
                self.expr(value);
                self.match_arms(arms)
            }
            StmtKind::Break | StmtKind::Continue => true,
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
                true
            }
            StmtKind::Expr(e) => {
                self.expr(e);
                stmt.never_returns()
            }
        }
    }

    /// The body may run zero times, so its assignments do not count after.
    fn for_body(&mut self, var: &'a Ident, body: &'a Block) {
        let before = self.init.clone();
        self.scopes.push(HashMap::new());
        self.declare(var, true);
        self.block(body);
        self.scopes.pop();
        self.init = before;
    }

    /// Runs every arm from the same starting state. After the `match` a
    /// variable is assigned if every arm that gets there assigned it. Returns
    /// `true` if no arm gets there.
    fn match_arms(&mut self, arms: &'a [MatchArm]) -> bool {
        let before = self.init.clone();
        let mut merged: Option<Vec<bool>> = None;

        for arm in arms {
            self.init.clone_from(&before);
            self.scopes.push(HashMap::new());
            for name in arm.pattern.bindings() {
                self.declare(name, true);
            }
            let diverges = self.block(&arm.body);
            self.scopes.pop();
            self.init.truncate(before.len());

            if !diverges {
                merged = Some(match merged {
                    None => self.init.clone(),
                    Some(so_far) => so_far
                        .iter()
                        .zip(&self.init)
                        .map(|(a, b)| *a && *b)
                        .collect(),
                });
            }
        }

        let diverges = merged.is_none() && !arms.is_empty();
        self.init = merged.unwrap_or(before);
        diverges
    }

    /// Runs both branches from the same starting state and merges the
    /// results. Returns `true` if both branches diverge.
    fn if_branches(&mut self, then: &'a Block, otherwise: Option<&'a Block>) -> bool {
        let before = self.init.clone();
        let then_diverges = self.block(then);
        let after_then = std::mem::replace(&mut self.init, before);

        let else_diverges = match otherwise {
            Some(block) => self.block(block),
            None => false,
        };
        // `self.init` now holds the state after the `else` branch.

        match (then_diverges, else_diverges) {
            (true, true) => return true,
            (true, false) => {}
            (false, true) => self.init = after_then,
            (false, false) => {
                for (now, then) in self.init.iter_mut().zip(&after_then) {
                    *now &= *then;
                }
            }
        }
        false
    }

    fn expr(&mut self, e: &'a Expr) {
        match &e.kind {
            ExprKind::Int(_)
            | ExprKind::Real(_)
            | ExprKind::Complex { .. }
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::None => {}
            ExprKind::Variant { args, .. } => {
                for arg in args {
                    self.expr(arg);
                }
            }
            ExprKind::Some(value) => self.expr(value),
            ExprKind::Lambda(l) => {
                let before = self.init.len();
                self.scopes.push(HashMap::new());
                for param in &l.params {
                    self.declare(&param.name, true);
                }
                self.block(&l.body);
                self.scopes.pop();
                self.init.truncate(before);
            }
            ExprKind::Var(name) => self.read(name),
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Call { args, .. } => {
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

    fn read(&mut self, name: &Ident) {
        let Some(id) = self.lookup(&name.name) else {
            return;
        };
        if !self.init[id] {
            self.errors.push(CheckError::new(
                format!("`{}` is used before being assigned", name.name),
                name.span,
            ));
            // Avoid repeating the same error for later reads.
            self.init[id] = true;
        }
    }
}
