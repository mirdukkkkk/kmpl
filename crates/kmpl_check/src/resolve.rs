//! Name resolution: every name refers to something that exists,
//! nothing is declared twice, constants are not assigned to.
//!
//! Rules:
//! - `fn` and `const` share one global namespace and may not reuse
//!   a built-in's name.
//! - Functions can be called before their definition.
//! - A constant can only use constants defined above it (no cycles).
//! - Parameters and `let` live in scopes. An inner block may shadow
//!   an outer name; the same block may not declare a name twice.

use std::collections::{HashMap, HashSet};

use kmpl_ast::{Block, Expr, ExprKind, Function, Ident, Item, Program, Stmt, StmtKind};

use crate::error::CheckError;

enum Global {
    /// `index` is the position of the item in the program.
    Const {
        index: usize,
    },
    Function,
}

struct Resolver<'a> {
    globals: HashMap<&'a str, Global>,
    /// Innermost scope is last.
    scopes: Vec<HashSet<&'a str>>,
    /// While checking a constant's initializer: that constant's index.
    current_const: Option<usize>,
    errors: Vec<CheckError>,
}

pub fn resolve(program: &Program) -> Vec<CheckError> {
    let mut r = Resolver {
        globals: HashMap::new(),
        scopes: Vec::new(),
        current_const: None,
        errors: Vec::new(),
    };

    r.declare_globals(program);

    for (index, item) in program.items.iter().enumerate() {
        match item {
            Item::Const(c) => {
                r.current_const = Some(index);
                r.expr(&c.value);
                r.current_const = None;
            }
            Item::Function(f) => r.function(f),
        }
    }

    r.errors
}

impl<'a> Resolver<'a> {
    fn error(&mut self, message: String, ident: &Ident) {
        self.errors.push(CheckError::new(message, ident.span));
    }

    fn declare_globals(&mut self, program: &'a Program) {
        for (index, item) in program.items.iter().enumerate() {
            let (name, global) = match item {
                Item::Const(c) => (&c.name, Global::Const { index }),
                Item::Function(f) => (&f.name, Global::Function),
            };

            if kmpl_builtins::lookup(&name.name).is_some() {
                self.error(format!("`{}` is a built-in function", name.name), name);
            } else if self.globals.contains_key(name.name.as_str()) {
                self.error(format!("`{}` is already defined", name.name), name);
            } else {
                self.globals.insert(&name.name, global);
            }
        }
    }

    fn function(&mut self, f: &'a Function) {
        self.scopes.push(HashSet::new());
        for param in &f.params {
            self.declare_local(&param.name);
        }
        self.block(&f.body);
        self.scopes.pop();
    }

    fn block(&mut self, block: &'a Block) {
        self.scopes.push(HashSet::new());
        for stmt in block {
            self.stmt(stmt);
        }
        self.scopes.pop();
    }

    fn declare_local(&mut self, name: &'a Ident) {
        let scope = self.scopes.last_mut().expect("inside a function");
        if !scope.insert(&name.name) {
            self.error(
                format!("`{}` is already defined in this scope", name.name),
                name,
            );
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains(name))
    }

    fn is_function(&self, name: &str) -> bool {
        matches!(self.globals.get(name), Some(Global::Function))
            || kmpl_builtins::lookup(name).is_some()
    }

    fn stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, init, .. } => {
                // The initializer is checked first, so `let x = x;`
                // refers to an outer `x`.
                if let Some(init) = init {
                    self.expr(init);
                }
                self.declare_local(name);
            }
            StmtKind::Assign { name, value } => {
                self.expr(value);
                self.assign_target(name);
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
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            StmtKind::Expr(e) => self.expr(e),
        }
    }

    fn assign_target(&mut self, name: &Ident) {
        let n = name.name.as_str();
        if self.is_local(n) {
            return;
        }

        let message = match self.globals.get(n) {
            Some(Global::Const { .. }) => format!("cannot assign to constant `{n}`"),
            _ if self.is_function(n) => format!("cannot assign to function `{n}`"),
            _ => format!("undefined variable `{n}`"),
        };
        self.error(message, name);
    }

    fn expr(&mut self, e: &'a Expr) {
        match &e.kind {
            ExprKind::Int(_) | ExprKind::Real(_) | ExprKind::Imag(_) | ExprKind::Bool(_) => {}
            ExprKind::Var(name) => self.var(name),
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Call { name, args } => {
                self.call_target(name);
                for arg in args {
                    self.expr(arg);
                }
            }
            ExprKind::Swizzle { base, .. } => self.expr(base),
        }
    }

    fn var(&mut self, name: &Ident) {
        let n = name.name.as_str();
        if self.is_local(n) {
            return;
        }

        let message = match self.globals.get(n) {
            Some(Global::Const { index }) => match self.current_const {
                Some(current) if *index >= current => {
                    format!("constant `{n}` is used before its definition")
                }
                _ => return,
            },
            _ if self.is_function(n) => {
                format!("`{n}` is a function, call it with `{n}(...)`")
            }
            _ => format!("undefined variable `{n}`"),
        };
        self.error(message, name);
    }

    fn call_target(&mut self, name: &Ident) {
        let n = name.name.as_str();
        if self.is_function(n) {
            return;
        }

        let message = if self.is_local(n) || self.globals.contains_key(n) {
            format!("`{n}` is not a function")
        } else {
            format!("undefined function `{n}`")
        };
        self.error(message, name);
    }
}
