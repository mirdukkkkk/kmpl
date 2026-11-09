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
//! - `stats.mean` needs `import stats from "...";` in the same file, and
//!   plain `mean` needs `import (mean) from "...";`. Imports are not
//!   transitive.
//! - A local variable hides a module of the same name: `stats.mean` then
//!   reads a field of the variable.
//! - A local variable hides an item of its own module, like it hides a
//!   constant in a single file. Calls ignore locals, as in `resolve`.
//! - The file being run has no prefix, so only its `main` is the program's.

use std::collections::{HashMap, HashSet};

use kmpl_ast::{
    Block, Expr, ExprKind, Function, Ident, ImportNames, Item, PathStep, Pattern, Program, Stmt,
    StmtKind, Type, TypeExpr,
};

use crate::error::CheckError;

pub struct Module {
    /// The full name, like `math.stats`. Empty for the file being run.
    pub prefix: String,
    pub imports: Vec<ModuleImport>,
    pub program: Program,
}

pub struct ModuleImport {
    /// The names the code uses.
    pub names: ImportNames,
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
        let own = &exports[&module.prefix];
        let mut names = ImportedNames::default();
        for import in &module.imports {
            names.add(import, own, &exports[&import.target], &mut errors);
        }

        let mut linker = Linker {
            prefix: &module.prefix,
            own,
            imports: &names.modules,
            items: &names.items,
            exports: &exports,
            scopes: Vec::new(),
            type_params: HashSet::new(),
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
            Item::Struct(s) => s.name.name.clone(),
            Item::Enum(e) => e.name.name.clone(),
        })
        .collect()
}

/// What the imports of one file bring into it.
#[derive(Default)]
struct ImportedNames {
    /// Name used in the code -> full name of the module.
    modules: HashMap<String, String>,
    /// Name used in the code -> full name of the item.
    items: HashMap<String, String>,
}

impl ImportedNames {
    fn add(
        &mut self,
        import: &ModuleImport,
        own: &HashSet<String>,
        exported: &HashSet<String>,
        errors: &mut Vec<CheckError>,
    ) {
        match &import.names {
            ImportNames::Module(name) => {
                if self.is_taken(&name.name) {
                    errors.push(already_imported(name));
                }
                self.modules
                    .insert(name.name.clone(), import.target.clone());
            }
            ImportNames::Items(list) => {
                for item in list {
                    if !exported.contains(&item.name.name) {
                        errors.push(CheckError::new(
                            format!("this module has no `{}`", item.name.name),
                            item.name.span,
                        ));
                    } else if self.is_taken(&item.alias.name) {
                        errors.push(already_imported(&item.alias));
                    } else if let Some(problem) = name_clash(&item.alias.name, own) {
                        errors.push(CheckError::new(problem, item.alias.span));
                    } else {
                        let full = format!("{}.{}", import.target, item.name.name);
                        self.items.insert(item.alias.name.clone(), full);
                    }
                }
            }
        }
    }

    fn is_taken(&self, name: &str) -> bool {
        self.modules.contains_key(name) || self.items.contains_key(name)
    }
}

fn already_imported(name: &Ident) -> CheckError {
    CheckError::new(format!("`{}` is already imported", name.name), name.span)
}

/// Why an imported item cannot be called `name` in this file, if it cannot.
fn name_clash(name: &str, own: &HashSet<String>) -> Option<String> {
    if own.contains(name) {
        Some(format!("`{name}` is already defined in this file"))
    } else if kmpl_builtins::lookup(name).is_some() {
        Some(format!("`{name}` is a built-in function"))
    } else if kmpl_builtins::lookup_const(name).is_some() {
        Some(format!("`{name}` is a built-in constant"))
    } else if Type::from_name(name).is_some() {
        Some(format!("`{name}` is a built-in type"))
    } else {
        None
    }
}

struct Linker<'a> {
    prefix: &'a str,
    /// Names of the items of this module.
    own: &'a HashSet<String>,
    /// Name used in the code -> full name of the module.
    imports: &'a HashMap<String, String>,
    /// Name used in the code -> full name of an imported item.
    items: &'a HashMap<String, String>,
    exports: &'a HashMap<String, HashSet<String>>,
    /// Local variables. Innermost scope is last.
    scopes: Vec<HashSet<String>>,
    /// Generic parameters of the current function. They hide structs.
    type_params: HashSet<String>,
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
                self.ty(&mut c.ty);
                self.expr(&mut c.value);
            }
            Item::Function(f) => self.function(f),
            Item::Struct(s) => {
                self.declared(&mut s.name);
                for field in &mut s.fields {
                    self.ty(&mut field.ty);
                }
            }
            Item::Enum(e) => {
                self.declared(&mut e.name);
                for variant in &mut e.variants {
                    for field in &mut variant.fields {
                        self.ty(field);
                    }
                }
            }
        }
    }

    fn function(&mut self, f: &mut Function) {
        self.declared(&mut f.name);
        self.type_params = f.generics.iter().map(|g| g.name.name.clone()).collect();

        for param in &mut f.params {
            self.ty(&mut param.ty);
        }
        if let Some(ret) = &mut f.ret {
            self.ty(ret);
        }

        let params = f.params.iter().map(|p| p.name.name.clone()).collect();
        self.scopes.push(params);
        self.block(&mut f.body);
        self.scopes.pop();
        self.type_params.clear();
    }

    /// Prefixes the name of an item. With a prefix `resolve` could no longer
    /// see that it reuses the name of a built-in, so that is checked here.
    fn declared(&mut self, name: &mut Ident) {
        if !self.prefix.is_empty() {
            if kmpl_builtins::lookup(&name.name).is_some() {
                self.error(format!("`{}` is a built-in function", name.name), name);
            } else if kmpl_builtins::lookup_const(&name.name).is_some() {
                self.error(format!("`{}` is a built-in constant", name.name), name);
            } else if Type::from_name(&name.name).is_some() {
                self.error(format!("`{}` is a built-in type", name.name), name);
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

    /// `stats.mean`, written in a file that imports `stats`.
    fn module_item(&mut self, name: &mut Ident) {
        let (module, member) = name.name.split_once('.').expect("a dotted name");
        let (module, member) = (module.to_owned(), member.to_owned());

        let Some(target) = self.imports.get(&module) else {
            self.error(
                format!("unknown module `{module}`, add `import {module} from \"...\";`"),
                name,
            );
            return;
        };
        if !self.exports[target].contains(&member) {
            self.error(format!("module `{module}` has no `{member}`"), name);
            return;
        }
        name.name = format!("{target}.{member}");
    }

    /// The name of a function or struct at a call or a literal: not a
    /// variable, so locals do not hide items.
    fn item_name(&mut self, name: &mut Ident) {
        let internal = name.name.starts_with('_') && kmpl_builtins::lookup(&name.name).is_some();
        if internal && !self.prefix.starts_with("std:") {
            self.error(
                format!("`{}` belongs to the standard library", name.name),
                name,
            );
            return;
        }

        if let Some((module, _)) = name.name.split_once('.') {
            if self.is_local(module) {
                let message = format!("`{module}` is a variable, not a module");
                self.error(message, name);
            } else {
                self.module_item(name);
            }
        } else {
            self.plain_item(name);
        }
    }

    /// `name` without a module: an item of this file or an imported one.
    fn plain_item(&mut self, name: &mut Ident) {
        if self.own.contains(&name.name) {
            name.name = self.full_name(&name.name);
        } else if let Some(full) = self.items.get(&name.name) {
            name.name = full.clone();
        }
    }

    /// A variable that is read or assigned to.
    fn variable(&mut self, name: &mut Ident) {
        if !self.is_local(&name.name) {
            self.plain_item(name);
        }
    }

    /// Locals do not hide types: they are different kinds of names.
    fn ty(&mut self, ty: &mut TypeExpr) {
        match ty {
            TypeExpr::Known(_) => {}
            TypeExpr::Array(elem) | TypeExpr::Optional(elem) => self.ty(elem),
            TypeExpr::Tuple(items) => {
                for item in items {
                    self.ty(item);
                }
            }
            TypeExpr::Param(name) if name.name.contains('.') => self.module_item(name),
            TypeExpr::Param(name) => {
                if !self.type_params.contains(&name.name) {
                    self.plain_item(name);
                }
            }
        }
    }

    /// Is `name` a module here? A local of that name hides it.
    fn is_module(&self, name: &str) -> bool {
        !self.is_local(name) && self.imports.contains_key(name)
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
            StmtKind::Let { name, ty, init } => {
                if let Some(ty) = ty {
                    self.ty(ty);
                }
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
            StmtKind::PathAssign {
                name, path, value, ..
            } => {
                self.expr(value);
                for step in path.iter_mut() {
                    if let PathStep::Index(index) = step {
                        self.expr(index);
                    }
                }

                // `stats.G[0] = 1;` starts with an item of a module.
                if self.is_module(&name.name)
                    && let Some(PathStep::Field(member)) = path.first()
                {
                    name.name = format!("{}.{}", name.name, member.name);
                    self.module_item(name);
                    path.remove(0);
                } else {
                    self.variable(name);
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
            StmtKind::Match { value, arms } => {
                self.expr(value);
                for arm in arms {
                    if let Pattern::Variant { name, .. } = &mut arm.pattern {
                        self.item_name(name);
                    }
                    let bindings = arm.pattern.bindings();
                    self.scopes
                        .push(bindings.iter().map(|n| n.name.clone()).collect());
                    self.block(&mut arm.body);
                    self.scopes.pop();
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
            | ExprKind::Str(_)
            | ExprKind::None => {}
            ExprKind::Variant { name, args, .. } => {
                self.item_name(name);
                for arg in args {
                    self.expr(arg);
                }
            }
            ExprKind::Some(value) => self.expr(value),
            ExprKind::Var(name) => self.variable(name),
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Call { name, args } => {
                self.item_name(name);
                for arg in args {
                    self.expr(arg);
                }
            }
            ExprKind::Swizzle { base, .. } => self.expr(base),
            ExprKind::Field { base, name } => {
                if let ExprKind::Var(first) = &base.kind {
                    let head = first.name.clone();
                    if self.is_module(&head) {
                        let mut item = Ident {
                            name: format!("{head}.{}", name.name),
                            span: e.span,
                        };
                        self.module_item(&mut item);
                        e.kind = ExprKind::Var(item);
                        return;
                    }

                    let is_known = self.is_local(&head)
                        || self.own.contains(&head)
                        || self.items.contains_key(&head)
                        || kmpl_builtins::lookup_const(&head).is_some();
                    if !is_known {
                        // A typo in a variable looks just like a missing import.
                        self.errors.push(CheckError::new(
                            format!("`{head}` is not a variable or an imported module"),
                            base.span,
                        ));
                        return;
                    }
                }
                self.expr(base);
            }
            ExprKind::StructLit { name, fields } => {
                self.item_name(name);
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
