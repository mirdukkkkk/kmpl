//! Joins the parsed files of a program into one `Program`.
//!
//! Every item of an imported module gets the full name of the module as a
//! prefix, so `mean` in `math/stats.kmpl` becomes `math.stats.mean`, and
//! every name that refers to an item is rewritten to match. A dot cannot be
//! part of an identifier, so these names never clash. After this the other
//! checks and the interpreter see one ordinary program, the same way they
//! only see the `lerp<vec2>` copies of a generic function.
//!
//! Rules:
//! - `stats.mean` needs `import ...stats;` in the same file. Imports are
//!   not transitive.
//! - A local variable hides a module of the same name: `stats.mean` is
//!   then an error, and will mean a field once structs exist.
//! - A local variable hides an item of its own module, like it hides a
//!   constant in a single file. Calls ignore locals, as in `resolve`.
//! - The file being run has no prefix, so only its `main` is the program's.

use std::collections::{HashMap, HashSet};

use kmpl_ast::{Block, Expr, ExprKind, Function, Ident, Item, Program, Stmt, StmtKind};

use crate::error::CheckError;

pub struct Module {
    /// The full name, like `math.stats`. Empty for the file being run.
    pub prefix: String,
    pub imports: Vec<ModuleImport>,
    pub program: Program,
}

pub struct ModuleImport {
    /// The name the code uses: the last part of the import path.
    pub name: Ident,
    /// `Module::prefix` of the imported module.
    pub target: String,
}

/// `modules` must list every imported module before the modules that import
/// it: a constant can only use what comes before it.
pub fn link(modules: Vec<Module>) -> Result<Program, Vec<CheckError>> {
    let exports: HashMap<String, HashSet<String>> = modules
        .iter()
        .map(|m| (m.prefix.clone(), item_names(&m.program)))
        .collect();

    let mut items = Vec::new();
    let mut errors = Vec::new();
    for mut module in modules {
        let mut imports = HashMap::new();
        for import in &module.imports {
            let name = &import.name;
            if imports
                .insert(name.name.clone(), import.target.clone())
                .is_some()
            {
                errors.push(CheckError::new(
                    format!("`{}` is already imported", name.name),
                    name.span,
                ));
            }
        }

        let mut linker = Linker {
            prefix: &module.prefix,
            own: &exports[&module.prefix],
            imports: &imports,
            exports: &exports,
            scopes: Vec::new(),
            errors: Vec::new(),
        };
        for item in &mut module.program.items {
            linker.item(item);
        }
        errors.append(&mut linker.errors);
        items.append(&mut module.program.items);
    }

    if errors.is_empty() {
        Ok(Program {
            imports: Vec::new(),
            items,
        })
    } else {
        Err(errors)
    }
}

fn item_names(program: &Program) -> HashSet<String> {
    program
        .items
        .iter()
        .map(|item| match item {
            Item::Const(c) => c.name.name.clone(),
            Item::Function(f) => f.name.name.clone(),
        })
        .collect()
}

struct Linker<'a> {
    prefix: &'a str,
    /// Names of the items of this module.
    own: &'a HashSet<String>,
    /// Name used in the code -> full name of the module.
    imports: &'a HashMap<String, String>,
    exports: &'a HashMap<String, HashSet<String>>,
    /// Local variables. Innermost scope is last.
    scopes: Vec<HashSet<String>>,
    errors: Vec<CheckError>,
}

impl Linker<'_> {
    fn error(&mut self, message: String, name: &Ident) {
        self.errors.push(CheckError::new(message, name.span));
    }

    fn full_name(&self, name: &str) -> String {
        if self.prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{}.{name}", self.prefix)
        }
    }

    fn item(&mut self, item: &mut Item) {
        match item {
            Item::Const(c) => {
                self.declared(&mut c.name);
                self.expr(&mut c.value);
            }
            Item::Function(f) => self.function(f),
        }
    }

    fn function(&mut self, f: &mut Function) {
        self.declared(&mut f.name);
        let params = f.params.iter().map(|p| p.name.name.clone()).collect();
        self.scopes.push(params);
        self.block(&mut f.body);
        self.scopes.pop();
    }

    /// Prefixes the name of an item. With a prefix `resolve` could no longer
    /// see that it reuses the name of a built-in, so that is checked here.
    fn declared(&mut self, name: &mut Ident) {
        if !self.prefix.is_empty() {
            if kmpl_builtins::lookup(&name.name).is_some() {
                self.error(format!("`{}` is a built-in function", name.name), name);
            } else if kmpl_builtins::lookup_const(&name.name).is_some() {
                self.error(format!("`{}` is a built-in constant", name.name), name);
            }
        }
        name.name = self.full_name(&name.name);
    }

    fn declare(&mut self, name: &str) {
        self.scopes
            .last_mut()
            .expect("inside a function")
            .insert(name.to_owned());
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains(name))
    }

    /// `stats.mean`, written in a file that imports `stats`. Returns
    /// whether `name` had a dot, so was handled here.
    fn qualified(&mut self, name: &mut Ident) -> bool {
        let Some((module, member)) = name.name.split_once('.') else {
            return false;
        };
        let (module, member) = (module.to_owned(), member.to_owned());

        if self.is_local(&module) {
            self.error(format!("`{module}` is a variable, not a module"), name);
            return true;
        }
        let Some(target) = self.imports.get(&module) else {
            self.error(
                format!("unknown module `{module}`, add `import {module};`"),
                name,
            );
            return true;
        };
        if !self.exports[target].contains(&member) {
            self.error(format!("module `{module}` has no `{member}`"), name);
            return true;
        }

        name.name = format!("{target}.{member}");
        true
    }

    /// A variable that is read or assigned to.
    fn variable(&mut self, name: &mut Ident) {
        if !self.qualified(name) && !self.is_local(&name.name) && self.own.contains(&name.name) {
            name.name = self.full_name(&name.name);
        }
    }

    fn block(&mut self, block: &mut Block) {
        self.scopes.push(HashSet::new());
        for stmt in block {
            self.stmt(stmt);
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.kind {
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
            StmtKind::Assign { name, value, .. } => {
                self.expr(value);
                self.variable(name);
            }
            StmtKind::IndexAssign {
                name,
                indices,
                value,
                ..
            } => {
                self.expr(value);
                for index in indices {
                    self.expr(index);
                }
                self.variable(name);
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
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            StmtKind::Expr(e) => self.expr(e),
        }
    }

    fn loop_body(&mut self, var: &Ident, body: &mut Block) {
        self.scopes.push(HashSet::from([var.name.clone()]));
        self.block(body);
        self.scopes.pop();
    }

    fn expr(&mut self, e: &mut Expr) {
        match &mut e.kind {
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
                if !self.qualified(name) && self.own.contains(&name.name) {
                    name.name = self.full_name(&name.name);
                }
                for arg in args {
                    self.expr(arg);
                }
            }
            ExprKind::Swizzle { base, .. } => self.expr(base),
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
