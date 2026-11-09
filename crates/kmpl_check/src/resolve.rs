//! Name resolution and structural checks, in one walk over the program:
//! every name refers to something that exists, nothing is declared twice,
//! constants are not assigned to, calls pass the right number of arguments,
//! `break` and `continue` are inside a loop.
//!
//! Rules:
//! - `fn` and `const` share one global namespace and may not reuse
//!   a built-in's name.
//! - Functions can be called before their definition.
//! - A constant can only use constants defined above it (no cycles) and
//!   call built-in functions: a user function could read a constant that
//!   is not computed yet.
//! - A type name that is not built in must be a generic parameter of the
//!   enclosing function (`fn f<T>(x: T)`); generic names are unique.
//! - Parameters and `let` live in scopes. An inner block may shadow
//!   an outer name; the same block may not declare a name twice.
//! - A `for` variable lives in its own scope around the body and cannot be
//!   assigned to. The body may shadow it with `let`.

use std::collections::{HashMap, HashSet};

use kmpl_ast::{
    Block, EnumDef, Expr, ExprKind, Function, Ident, Item, Lambda, Pattern, Program, Span, Stmt,
    StmtKind, StructDef, Type, TypeExpr,
};

use crate::error::CheckError;
use crate::returns::always_returns;

enum Global<'a> {
    /// `index` is the position of the item in the program.
    Const {
        index: usize,
    },
    Function {
        params: usize,
    },
    Struct {
        fields: Vec<&'a str>,
        /// How many type arguments it takes.
        generics: usize,
    },
    Enum {
        /// The name of every variant and how many values it holds.
        variants: Vec<(&'a str, usize)>,
        generics: usize,
    },
}

struct Resolver<'a> {
    globals: HashMap<&'a str, Global<'a>>,
    /// Name -> is it a `for` variable? Innermost scope is last.
    scopes: Vec<HashMap<&'a str, bool>>,
    /// While checking a constant's initializer: that constant's index.
    current_const: Option<usize>,
    /// How many loops enclose the current statement.
    loop_depth: usize,
    /// Generic parameter names of the function being checked.
    type_params: Vec<&'a str>,
    /// Inside a function without a name: how many scopes are outside it.
    /// Those variables can only be read.
    captured_scopes: usize,
    errors: Vec<CheckError>,
}

pub fn resolve(program: &Program) -> Vec<CheckError> {
    let mut r = Resolver {
        globals: HashMap::new(),
        scopes: Vec::new(),
        current_const: None,
        loop_depth: 0,
        type_params: Vec::new(),
        captured_scopes: 0,
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
            Item::Struct(s) => r.struct_def(s),
            Item::Enum(e) => r.enum_def(e),
        }
    }
    r.recursive_structs(program);

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
                Item::Struct(s) => (
                    &s.name,
                    Global::Struct {
                        fields: s.fields.iter().map(|f| f.name.name.as_str()).collect(),
                        generics: s.generics.len(),
                    },
                ),
                Item::Enum(e) => (
                    &e.name,
                    Global::Enum {
                        variants: e
                            .variants
                            .iter()
                            .map(|v| (v.name.name.as_str(), v.fields.len()))
                            .collect(),
                        generics: e.generics.len(),
                    },
                ),
            };

            if kmpl_builtins::lookup(&name.name).is_some() {
                self.error(format!("`{}` is a built-in function", name.name), name);
            } else if kmpl_builtins::lookup_const(&name.name).is_some() {
                self.error(format!("`{}` is a built-in constant", name.name), name);
            } else if Type::from_name(&name.name).is_some() {
                self.error(format!("`{}` is a built-in type", name.name), name);
            } else if self.globals.contains_key(name.name.as_str()) {
                self.error(format!("`{}` is already defined", name.name), name);
            } else {
                self.globals.insert(&name.name, global);
            }
        }
    }

    /// Makes these the generic names in scope, reporting duplicates.
    fn enter_generics(&mut self, generics: impl Iterator<Item = &'a Ident>) {
        self.type_params.clear();
        for ident in generics {
            let name = ident.name.as_str();
            if self.type_params.contains(&name) {
                self.error(format!("`{name}` is already defined"), ident);
            } else {
                self.type_params.push(name);
            }
        }
    }

    fn struct_def(&mut self, s: &'a StructDef) {
        self.enter_generics(s.generics.iter());
        let mut seen = HashSet::new();
        for field in &s.fields {
            if !seen.insert(field.name.name.as_str()) {
                self.error(
                    format!(
                        "field `{}` is already defined in `{}`",
                        field.name.name, s.name.name
                    ),
                    &field.name,
                );
            }
            self.check_type(&field.ty);
        }
        self.type_params.clear();
    }

    fn enum_def(&mut self, e: &'a EnumDef) {
        self.enter_generics(e.generics.iter());
        let mut seen = HashSet::new();
        for variant in &e.variants {
            if !seen.insert(variant.name.name.as_str()) {
                self.error(
                    format!(
                        "variant `{}` is already defined in `{}`",
                        variant.name.name, e.name.name
                    ),
                    &variant.name,
                );
            }
            for field in &variant.fields {
                self.check_type(field);
            }
        }
        self.type_params.clear();
    }

    /// How many values `name::variant` holds, or `None` after reporting
    /// that there is no such thing.
    fn variant_size(&mut self, name: &Ident, variant: &Ident) -> Option<usize> {
        let n = name.name.as_str();
        let Some(Global::Enum { variants, .. }) = self.globals.get(n) else {
            let message = if self.globals.contains_key(n) {
                format!("`{n}` is not an enum")
            } else {
                format!("unknown enum `{n}`")
            };
            self.error(message, name);
            return None;
        };
        let found = variants.iter().find(|(v, _)| *v == variant.name);
        let size = found.map(|(_, size)| *size);
        if size.is_none() {
            self.error(format!("`{n}` has no variant `{}`", variant.name), variant);
        }
        size
    }

    /// `struct A { b: B }` with a `B` that holds an `A` has no finite size.
    /// An array can be empty, so it ends the chain.
    fn recursive_structs(&mut self, program: &'a Program) {
        let structs: HashMap<&str, &StructDef> = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(s) => Some((s.name.name.as_str(), s)),
                _ => None,
            })
            .collect();

        for item in &program.items {
            let Item::Struct(s) = item else { continue };
            let name = s.name.name.as_str();
            if reaches(s, name, &structs, &mut HashSet::new()) {
                self.error(
                    format!("`{name}` contains itself, so it would be infinitely large; use an array of `{name}`"),
                    &s.name,
                );
            }
        }
    }

    fn function(&mut self, f: &'a Function) {
        self.enter_generics(f.generics.iter().map(|g| &g.name));

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

    /// How many type arguments the struct or enum `name` takes, if it is one.
    fn takes_arguments(&self, name: &str) -> Option<usize> {
        match self.globals.get(name) {
            Some(Global::Struct { generics, .. } | Global::Enum { generics, .. }) => {
                Some(*generics)
            }
            _ => None,
        }
    }

    /// A non-built-in type name must be a generic parameter in scope.
    fn check_type(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::Param(name) if !self.type_params.contains(&name.name.as_str()) => {
                match self.takes_arguments(&name.name) {
                    None => self.error(format!("unknown type `{}`", name.name), name),
                    Some(0) => {}
                    Some(n) => self.error(
                        format!(
                            "`{}` needs {n} type arguments: `{}<...>`",
                            name.name, name.name
                        ),
                        name,
                    ),
                }
            }
            TypeExpr::Applied { name, args } => {
                match self.takes_arguments(&name.name) {
                    None => self.error(format!("unknown type `{}`", name.name), name),
                    Some(n) if n != args.len() => self.error(
                        format!(
                            "`{}` takes {n} type arguments, but {} were given",
                            name.name,
                            args.len()
                        ),
                        name,
                    ),
                    Some(_) => {}
                }
                for arg in args {
                    self.check_type(arg);
                }
            }
            TypeExpr::Array(elem) | TypeExpr::Optional(elem) => self.check_type(elem),
            TypeExpr::Function { params, ret } => {
                for param in params {
                    self.check_type(param);
                }
                if let Some(ret) = ret {
                    self.check_type(ret);
                }
            }
            TypeExpr::Tuple(items) => {
                for item in items {
                    self.check_type(item);
                }
            }
            _ => {}
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
            StmtKind::LetTuple { names, value } => {
                self.expr(value);
                for name in names.iter().filter(|n| n.name != "_") {
                    self.declare_local(name);
                }
            }
            StmtKind::Assign { name, value, .. } => {
                self.expr(value);
                self.assign_target(name);
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
                step,
                body,
                ..
            } => {
                // The bounds come first, so `for i in 0..i` uses an outer `i`.
                self.expr(start);
                self.expr(end);
                if let Some(step) = step {
                    self.expr(step);
                }
                self.for_body(var, body);
            }
            StmtKind::ForEach { var, array, body } => {
                self.expr(array);
                self.for_body(var, body);
            }
            StmtKind::Match { value, arms } => {
                self.expr(value);
                for arm in arms {
                    if let Pattern::Variant {
                        name,
                        variant,
                        bindings,
                    } = &arm.pattern
                        && let Some(size) = self.variant_size(name, variant)
                        && size != bindings.len()
                    {
                        self.error(
                            format!(
                                "`{}::{}` holds {size} values, but the pattern names {}",
                                name.name,
                                variant.name,
                                bindings.len()
                            ),
                            variant,
                        );
                    }
                    self.scopes.push(HashMap::new());
                    for name in arm.pattern.bindings().iter().filter(|n| n.name != "_") {
                        self.declare_local(name);
                    }
                    self.block(&arm.body);
                    self.scopes.pop();
                }
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

    fn for_body(&mut self, var: &'a Ident, body: &'a Block) {
        self.scopes.push(HashMap::from([(var.name.as_str(), true)]));
        self.loop_depth += 1;
        self.block(body);
        self.loop_depth -= 1;
        self.scopes.pop();
    }

    fn lambda(&mut self, l: &'a Lambda, span: Span) {
        // Copies of generic functions do not reach inside a body like this.
        let type_params = std::mem::take(&mut self.type_params);
        for param in &l.params {
            self.check_type(&param.ty);
        }
        if let Some(ret) = &l.ret {
            self.check_type(ret);
            if !always_returns(&l.body) {
                self.errors.push(CheckError::new(
                    "this function may finish without returning a value".to_owned(),
                    span,
                ));
            }
        }

        let captured = std::mem::replace(&mut self.captured_scopes, self.scopes.len());
        let loops = std::mem::replace(&mut self.loop_depth, 0);
        self.scopes.push(HashMap::new());
        for param in &l.params {
            self.declare_local(&param.name);
        }
        self.block(&l.body);
        self.scopes.pop();
        self.loop_depth = loops;
        self.captured_scopes = captured;
        self.type_params = type_params;
    }

    fn assign_target(&mut self, name: &Ident) {
        let n = name.name.as_str();
        let scope = self.scopes.iter().rposition(|s| s.contains_key(n));
        if scope.is_some_and(|index| index < self.captured_scopes) {
            self.error(
                format!("cannot change `{n}`: a function only reads what it captures"),
                name,
            );
            return;
        }
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
            Some(Global::Struct { .. }) => format!("cannot assign to struct `{n}`"),
            Some(Global::Enum { .. }) => format!("cannot assign to enum `{n}`"),
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
            ExprKind::Int(_)
            | ExprKind::Real(_)
            | ExprKind::Complex { .. }
            | ExprKind::Bool(_)
            | ExprKind::Str(_) => {}
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
            ExprKind::Swizzle { base, .. } | ExprKind::Field { base, .. } => self.expr(base),
            ExprKind::StructLit { name, fields } => self.struct_literal(name, fields),
            ExprKind::Variant {
                name,
                variant,
                args,
            } => {
                for arg in args {
                    self.expr(arg);
                }
                if let Some(size) = self.variant_size(name, variant)
                    && size != args.len()
                {
                    self.error(
                        format!(
                            "`{}::{}` holds {size} values, but {} were given",
                            name.name,
                            variant.name,
                            args.len()
                        ),
                        variant,
                    );
                }
            }
            ExprKind::Some(value) => self.expr(value),
            ExprKind::None => {}
            ExprKind::Lambda(l) => self.lambda(l, e.span),
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

    fn struct_literal(&mut self, name: &'a Ident, fields: &'a [(Ident, Expr)]) {
        for (_, value) in fields {
            self.expr(value);
        }

        let n = name.name.as_str();
        let Some(Global::Struct {
            fields: declared, ..
        }) = self.globals.get(n)
        else {
            let known = self.globals.contains_key(n) || kmpl_builtins::lookup(n).is_some();
            let message = if known {
                format!("`{n}` is not a struct")
            } else {
                format!("unknown struct `{n}`")
            };
            self.error(message, name);
            return;
        };
        let declared = declared.clone();

        let mut seen = HashSet::new();
        for (field, _) in fields {
            let f = field.name.as_str();
            if !declared.contains(&f) {
                self.error(format!("`{n}` has no field `{f}`"), field);
            } else if !seen.insert(f) {
                self.error(format!("field `{f}` is specified twice"), field);
            }
        }

        let missing: Vec<String> = declared
            .iter()
            .filter(|f| !seen.contains(*f))
            .map(|f| format!("`{f}`"))
            .collect();
        if !missing.is_empty() {
            let word = if missing.len() == 1 {
                "field"
            } else {
                "fields"
            };
            self.error(
                format!("missing {word} {} in `{n}`", missing.join(", ")),
                name,
            );
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
            Some(Global::Struct { .. }) => format!("`{n}` is a struct, not a value"),
            Some(Global::Enum { .. }) => format!("`{n}` is an enum, not a value"),
            _ if kmpl_builtins::lookup_const(n).is_some() => return,
            Some(Global::Function { .. }) => return,
            _ if self.is_function(n) => {
                format!("the built-in `{n}` cannot be a value, wrap it in `fn(...) {{ ... }}`")
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
        } else if self.is_local(n) || matches!(self.globals.get(n), Some(Global::Const { .. })) {
            // It may hold a function, which only the types can tell.
            return;
        } else if self.globals.contains_key(n) || kmpl_builtins::lookup_const(n).is_some() {
            format!("`{n}` is not a function")
        } else {
            format!("undefined function `{n}`")
        };
        self.error(message, name);
    }
}

/// Does a value of struct `s` hold a `target` inside it?
fn reaches<'a>(
    s: &'a StructDef,
    target: &str,
    structs: &HashMap<&str, &'a StructDef>,
    visited: &mut HashSet<&'a str>,
) -> bool {
    let mut held = Vec::new();
    for field in &s.fields {
        held_structs(&field.ty, &mut held);
    }

    held.into_iter().any(|name| {
        name == target
            || (visited.insert(name)
                && structs
                    .get(name)
                    .is_some_and(|next| reaches(next, target, structs, visited)))
    })
}

/// Names of the structs a value of type `ty` always contains.
fn held_structs<'a>(ty: &'a TypeExpr, out: &mut Vec<&'a str>) {
    match ty {
        TypeExpr::Param(name) => out.push(&name.name),
        TypeExpr::Applied { name, args } => {
            out.push(&name.name);
            for arg in args {
                held_structs(arg, out);
            }
        }
        TypeExpr::Tuple(items) => {
            for item in items {
                held_structs(item, out);
            }
        }
        // These can be empty, so they end the chain.
        TypeExpr::Known(_)
        | TypeExpr::Array(_)
        | TypeExpr::Optional(_)
        | TypeExpr::Function { .. } => {}
    }
}
