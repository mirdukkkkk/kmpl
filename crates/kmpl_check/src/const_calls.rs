//! A constant may call any function, as long as that is safe.
//!
//! Constants are computed in order before `main`, so a call from one of them
//! must not reach a constant that has no value yet, or anything that depends
//! on how the program is run. Everything a call can reach is followed here.
//! Runs after `resolve`, so every name exists.

use std::collections::{HashMap, HashSet};

use kmpl_ast::{Block, Expr, ExprKind, Function, Ident, Item, PathStep, Program, Stmt, StmtKind};

use crate::error::CheckError;

pub fn check_const_calls(program: &Program) -> Vec<CheckError> {
    let mut functions = HashMap::new();
    let mut constants = HashMap::new();
    for (index, item) in program.items.iter().enumerate() {
        match item {
            Item::Function(f) => {
                functions.insert(f.name.name.as_str(), f);
            }
            Item::Const(c) => {
                constants.insert(c.name.name.as_str(), index);
            }
            Item::Struct(_) => {}
        }
    }

    let mut walk = Walk {
        functions: &functions,
        constants: &constants,
        computed_before: 0,
        direct: None,
        visited: HashSet::new(),
        scopes: Vec::new(),
        found: false,
        errors: Vec::new(),
    };
    for (index, item) in program.items.iter().enumerate() {
        if let Item::Const(c) = item {
            walk.computed_before = index;
            walk.expr(&c.value);
        }
    }
    walk.errors
}

struct Walk<'a> {
    functions: &'a HashMap<&'a str, &'a Function>,
    constants: &'a HashMap<&'a str, usize>,
    /// The position of the constant being computed: only earlier ones have
    /// a value.
    computed_before: usize,
    /// The call written in the constant that led here. `None` while still in
    /// the expression of the constant itself, where `resolve` checks names.
    direct: Option<&'a Ident>,
    visited: HashSet<&'a str>,
    /// Local variables of the function being followed.
    scopes: Vec<HashSet<&'a str>>,
    /// One error per call written in a constant is enough.
    found: bool,
    errors: Vec<CheckError>,
}

impl<'a> Walk<'a> {
    fn report(&mut self, reason: String) {
        if self.found {
            return;
        }
        self.found = true;
        let direct = self.direct.expect("reported from a call");
        self.errors.push(CheckError::new(
            format!("a constant cannot call `{}`: {reason}", direct.name),
            direct.span,
        ));
    }

    fn call(&mut self, name: &'a Ident) {
        let outermost = self.direct.is_none();
        if outermost {
            self.direct = Some(name);
            self.visited.clear();
            self.found = false;
        }

        if kmpl_builtins::depends_on_run(&name.name) {
            self.report("its value depends on how the program is run".to_owned());
        } else if let Some(function) = self.functions.get(name.name.as_str()).copied()
            && self.visited.insert(&name.name)
        {
            self.function(function);
        }

        if outermost {
            self.direct = None;
        }
    }

    fn function(&mut self, function: &'a Function) {
        // A function has its own locals, whoever calls it.
        let outer = std::mem::take(&mut self.scopes);
        self.scopes.push(
            function
                .params
                .iter()
                .map(|p| p.name.name.as_str())
                .collect(),
        );
        self.block(&function.body);
        self.scopes = outer;
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains(name))
    }

    fn declare(&mut self, name: &'a str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name);
        }
    }

    fn block(&mut self, block: &'a Block) {
        self.scopes.push(HashSet::new());
        for stmt in block {
            self.stmt(stmt);
        }
        self.scopes.pop();
    }

    fn loop_body(&mut self, var: &'a Ident, body: &'a Block) {
        self.scopes.push(HashSet::from([var.name.as_str()]));
        self.block(body);
        self.scopes.pop();
    }

    fn stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, init, .. } => {
                // The initializer comes first: `let x = x;` sees the outer `x`.
                if let Some(init) = init {
                    self.expr(init);
                }
                self.declare(&name.name);
            }
            StmtKind::LetTuple { names, value } => {
                self.expr(value);
                for name in names {
                    self.declare(&name.name);
                }
            }
            StmtKind::Assign { value, .. } => self.expr(value),
            StmtKind::PathAssign { path, value, .. } => {
                self.expr(value);
                for step in path {
                    if let PathStep::Index(index) = step {
                        self.expr(index);
                    }
                }
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
                self.loop_body(var, body);
            }
            StmtKind::ForEach { var, array, body } => {
                self.expr(array);
                self.loop_body(var, body);
            }
            StmtKind::Return(Some(value)) | StmtKind::Expr(value) => self.expr(value),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
        }
    }

    fn variable(&mut self, name: &'a Ident) {
        if self.direct.is_none() || self.is_local(&name.name) {
            return;
        }
        if self
            .constants
            .get(name.name.as_str())
            .is_some_and(|index| *index >= self.computed_before)
        {
            self.report(format!("it reads `{}`, which has no value yet", name.name));
        }
    }

    fn expr(&mut self, e: &'a Expr) {
        match &e.kind {
            ExprKind::Int(_)
            | ExprKind::Real(_)
            | ExprKind::Complex { .. }
            | ExprKind::Bool(_)
            | ExprKind::Str(_) => {}
            ExprKind::Var(name) => self.variable(name),
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Call { name, args } => {
                for arg in args {
                    self.expr(arg);
                }
                self.call(name);
            }
            ExprKind::Swizzle { base, .. } | ExprKind::Field { base, .. } => self.expr(base),
            ExprKind::StructLit { fields, .. } => {
                for (_, value) in fields {
                    self.expr(value);
                }
            }
            ExprKind::Tuple(items) | ExprKind::Array(items) => {
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
