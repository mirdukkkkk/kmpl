//! Name resolution and structural checks, in one walk over the program:
//! every name refers to something that exists, nothing is declared twice,
//! constants are not assigned to, calls pass the right number of arguments,
//! `break` and `continue` are inside a loop.
//!
//! Rules:
//! - `fn` and `const` share one global namespace and may not reuse
//!   a built-in's name.
//! - Functions can be called before their definition.
//! - A constant can only use constants defined above it (no cycles).
//! - A type name that is not built in must be a generic parameter of the
//!   enclosing function (`fn f<T>(x: T)`); generic names are unique.
//! - Parameters and `let` live in scopes. An inner block may shadow
//!   an outer name; the same block may not declare a name twice.
//! - A `for` variable lives in its own scope around the body and cannot be
//!   assigned to. The body may shadow it with `let`.

use std::collections::HashMap;

use kmpl_ast::{Block, Expr, ExprKind, Function, Ident, Item, Program, Stmt, StmtKind, TypeExpr};

use crate::error::CheckError;

enum Global {
    /// `index` is the position of the item in the program.
    Const {
        index: usize,
    },
    Function {
        params: usize,
    },
}

struct Resolver<'a> {
    globals: HashMap<&'a str, Global>,
    /// Name -> is it a `for` variable? Innermost scope is last.
    scopes: Vec<HashMap<&'a str, bool>>,
    /// While checking a constant's initializer: that constant's index.
    current_const: Option<usize>,
    /// How many loops enclose the current statement.
    loop_depth: usize,
    /// Generic parameter names of the function being checked.
    type_params: Vec<&'a str>,
    errors: Vec<CheckError>,
}

pub fn resolve(program: &Program) -> Vec<CheckError> {
    let mut r = Resolver {
        globals: HashMap::new(),
        scopes: Vec::new(),
        current_const: None,
        loop_depth: 0,
        type_params: Vec::new(),
        errors: Vec::new(),
    };

    r.declare_globals(program);

    for (index, item) in program.items.iter().enumerate() {
        match item {
            Item::Const(c) => {
                r.check_type(&c.ty);
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
                Item::Function(f) => (
                    &f.name,
                    Global::Function {
                        params: f.params.len(),
                    },
                ),
            };

            if kmpl_builtins::lookup(&name.name).is_some() {
                self.error(format!("`{}` is a built-in function", name.name), name);
            } else if kmpl_builtins::lookup_const(&name.name).is_some() {
                self.error(format!("`{}` is a built-in constant", name.name), name);
            } else if self.globals.contains_key(name.name.as_str()) {
                self.error(format!("`{}` is already defined", name.name), name);
            } else {
                self.globals.insert(&name.name, global);
            }
        }
    }

    fn function(&mut self, f: &'a Function) {
        self.type_params.clear();
        for generic in &f.generics {
            let name = generic.name.name.as_str();
            if self.type_params.contains(&name) {
                self.error(format!("`{name}` is already defined"), &generic.name);
            } else {
                self.type_params.push(name);
            }
        }

        self.scopes.push(HashMap::new());
        for param in &f.params {
            self.check_type(&param.ty);
            self.declare_local(&param.name);
        }
        if let Some(ret) = &f.ret {
            self.check_type(ret);
        }
        self.block(&f.body);
        self.scopes.pop();
        self.type_params.clear();
    }

    /// A non-built-in type name must be a generic parameter in scope.
    fn check_type(&mut self, ty: &TypeExpr) {
        if let TypeExpr::Param(name) = ty
            && !self.type_params.contains(&name.name.as_str())
        {
            self.error(format!("unknown type `{}`", name.name), name);
        }
    }

    fn block(&mut self, block: &'a Block) {
        self.scopes.push(HashMap::new());
        for stmt in block {
            self.stmt(stmt);
        }
        self.scopes.pop();
    }

    fn declare_local(&mut self, name: &'a Ident) {
        let scope = self.scopes.last_mut().expect("inside a function");
        if scope.insert(&name.name, false).is_some() {
            self.error(
                format!("`{}` is already defined in this scope", name.name),
                name,
            );
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains_key(name))
    }

    /// Number of parameters, if `name` is a user or built-in function.
    fn param_count(&self, name: &str) -> Option<usize> {
        match self.globals.get(name) {
            Some(Global::Function { params }) => Some(*params),
            _ => kmpl_builtins::lookup(name).map(|b| b.params.len()),
        }
    }

    fn is_function(&self, name: &str) -> bool {
        self.param_count(name).is_some()
    }

    fn stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, ty, init } => {
                if let Some(ty) = ty {
                    self.check_type(ty);
                }
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
                self.loop_depth += 1;
                self.block(body);
                self.loop_depth -= 1;
            }
            StmtKind::For {
                var,
                start,
                end,
                body,
            } => {
                // The bounds come first, so `for i in 0..i` uses an outer `i`.
                self.expr(start);
                self.expr(end);
                self.scopes.push(HashMap::from([(var.name.as_str(), true)]));
                self.loop_depth += 1;
                self.block(body);
                self.loop_depth -= 1;
                self.scopes.pop();
            }
            StmtKind::Break | StmtKind::Continue => {
                if self.loop_depth == 0 {
                    let word = match stmt.kind {
                        StmtKind::Break => "break",
                        _ => "continue",
                    };
                    self.errors.push(CheckError::new(
                        format!("`{word}` outside of a loop"),
                        stmt.span,
                    ));
                }
            }
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
        let local = self.scopes.iter().rev().find_map(|s| s.get(n).copied());
        match local {
            Some(true) => {
                self.error(format!("cannot assign to loop variable `{n}`"), name);
                return;
            }
            Some(false) => return,
            None => {}
        }

        let message = match self.globals.get(n) {
            Some(Global::Const { .. }) => format!("cannot assign to constant `{n}`"),
            _ if kmpl_builtins::lookup_const(n).is_some() => {
                format!("cannot assign to constant `{n}`")
            }
            _ if self.is_function(n) => format!("cannot assign to function `{n}`"),
            _ => format!("undefined variable `{n}`"),
        };
        self.error(message, name);
    }

    fn expr(&mut self, e: &'a Expr) {
        match &e.kind {
            ExprKind::Int(_) | ExprKind::Real(_) | ExprKind::Complex { .. } | ExprKind::Bool(_) => {
            }
            ExprKind::Var(name) => self.var(name),
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Call { name, args } => {
                self.call_target(name, args.len());
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
            _ if kmpl_builtins::lookup_const(n).is_some() => return,
            _ if self.is_function(n) => {
                format!("`{n}` is a function, call it with `{n}(...)`")
            }
            _ => format!("undefined variable `{n}`"),
        };
        self.error(message, name);
    }

    fn call_target(&mut self, name: &Ident, given: usize) {
        let n = name.name.as_str();

        let message = if let Some(expected) = self.param_count(n) {
            if expected == given {
                return;
            }
            format!(
                "`{n}` takes {expected} {}, but {given} {} given",
                if expected == 1 {
                    "argument"
                } else {
                    "arguments"
                },
                if given == 1 { "was" } else { "were" },
            )
        } else if self.is_local(n)
            || self.globals.contains_key(n)
            || kmpl_builtins::lookup_const(n).is_some()
        {
            format!("`{n}` is not a function")
        } else {
            format!("undefined function `{n}`")
        };
        self.error(message, name);
    }
}
