//! Type inference by unification. See `types.rs` for type variables.
//!
//! Each expression gets a type variable. Operators, calls, `let` and
//! assignments add constraints by unifying variables or narrowing their
//! sets. After the whole program is walked, variables that still have
//! several candidates get a default (`int`, then `real`, then `complex`).
//!
//! `*`, `/` and swizzles must know whether an operand is a number, a vector
//! or an array when they are reached. In practice it always is, because a
//! variable cannot be read before it is assigned.
//!
//! Runs only on programs without name errors, so every name exists and
//! every call has the right number of arguments. Generic functions are
//! skipped: their monomorphized copies (`generics.rs`) are checked instead,
//! and a call to a generic function picks its copy from the inferred types.

use std::collections::{HashMap, HashSet};

use kmpl_ast::{
    BinOp, Block, EnumDef, Expr, ExprKind, Function, Ident, Item, Lambda, MatchArm, Operator,
    PathStep, Pattern, Program, Span, Stmt, StmtKind, StructDef, Ty, Type, TypeExpr, UnOp,
};
use kmpl_builtins::{ParamType, RetType};

use crate::error::CheckError;
use crate::generics::{instance_name, type_expr};
use crate::types::{TyVar, TypeSet, Unifier};

/// A call to a generic function, resolved to a copy at the end.
struct GenericCall<'a> {
    item: usize,
    span: Span,
    name: &'a str,
    /// One variable per generic parameter, in declaration order.
    vars: Vec<TyVar>,
}

struct Infer<'a> {
    u: Unifier,
    functions: HashMap<&'a str, &'a Function>,
    /// Every `operator` of the program, wherever it is defined.
    operators: Vec<&'a Function>,
    /// Where an operator of the program is used: the item, the span of the
    /// expression or of the assignment, and the function to call.
    operator_calls: Vec<(usize, Span, &'a str)>,
    consts: HashMap<&'a str, &'a TypeExpr>,
    /// Name of a struct or enum -> id in `Unifier`.
    struct_ids: HashMap<&'a str, usize>,
    struct_defs: HashMap<usize, &'a StructDef>,
    enum_defs: HashMap<usize, &'a EnumDef>,
    /// Name -> type of a local. Innermost scope is last.
    scopes: Vec<HashMap<&'a str, TyVar>>,
    /// Every `let` variable, to check at the end that its type is known.
    locals: Vec<(&'a Ident, TyVar)>,
    /// Every array literal, to check at the end that its element type is
    /// known: `print([]);` has no variable to report.
    array_literals: Vec<(Span, TyVar)>,
    /// Every `none`, for the same reason.
    nones: Vec<(Span, TyVar)>,
    /// Every value of a generic struct or enum, for the same reason:
    /// `Result::Ok(1)` says nothing about the type of the error.
    generic_values: Vec<(Span, TyVar)>,
    /// Every `1` and `1.0` literal with its item index, to pick its final
    /// type at the end. Copies of a generic function share spans, so the
    /// span alone is not unique.
    literals: Vec<(usize, Span, TyVar)>,
    /// Every expression, to record its final type at the end.
    expr_vars: Vec<(usize, Span, TyVar)>,
    generic_calls: Vec<GenericCall<'a>>,
    /// Index of the item being checked.
    item: usize,
    /// Errors already reported, so one mistake in a generic body is not
    /// repeated for every copy.
    reported: HashSet<(Span, String)>,
    /// The function being checked: its name and return type.
    current_fn: Option<(&'a str, Option<&'a TypeExpr>)>,
    errors: Vec<CheckError>,
}

pub struct Inference {
    pub errors: Vec<CheckError>,
    /// Final type of every integer and real literal, by item index and span.
    pub literal_types: HashMap<(usize, Span), Type>,
    /// The copy each call to a generic function uses, like `lerp<vec2>`,
    /// by item index and span of the call.
    pub call_targets: HashMap<(usize, Span), String>,
    /// The function behind each use of an operator the program defines, by
    /// item index and span of the expression, or of the whole assignment
    /// for `a += b`.
    pub operator_targets: HashMap<(usize, Span), String>,
    /// Calls to generic functions with arrays, structs and the like as type
    /// arguments, which have no copy made in advance.
    pub composite_calls: Vec<(String, Vec<Ty>)>,
    /// Final type of every expression, by item index and span.
    pub expr_types: HashMap<(usize, Span), Ty>,
}

/// `report_unknown`: report `let` variables whose type stays unknown.
/// Off when earlier checks already failed, since their errors (e.g. a
/// variable never assigned) usually cause these.
pub fn infer(program: &Program, report_unknown: bool) -> Inference {
    let mut inf = Infer {
        u: Unifier::default(),
        functions: HashMap::new(),
        operators: Vec::new(),
        operator_calls: Vec::new(),
        consts: HashMap::new(),
        struct_ids: HashMap::new(),
        struct_defs: HashMap::new(),
        enum_defs: HashMap::new(),
        scopes: Vec::new(),
        locals: Vec::new(),
        array_literals: Vec::new(),
        nones: Vec::new(),
        generic_values: Vec::new(),
        current_fn: None,
        literals: Vec::new(),
        expr_vars: Vec::new(),
        generic_calls: Vec::new(),
        item: 0,
        reported: HashSet::new(),
        errors: Vec::new(),
    };

    for item in &program.items {
        match item {
            Item::Const(c) => {
                inf.consts.insert(&c.name.name, &c.ty);
            }
            Item::Function(f) => {
                inf.functions.insert(&f.name.name, f);
                if f.operator.is_some() {
                    inf.operators.push(f);
                }
            }
            Item::Struct(s) => {
                let id = inf.u.add_struct(&s.name.name);
                inf.struct_ids.insert(&s.name.name, id);
                inf.struct_defs.insert(id, s);
            }
            Item::Enum(e) => {
                let id = inf.u.add_struct(&e.name.name);
                inf.struct_ids.insert(&e.name.name, id);
                inf.enum_defs.insert(id, e);
            }
        }
    }

    for (index, item) in program.items.iter().enumerate() {
        inf.item = index;
        match item {
            Item::Const(c) => {
                let value = inf.expr(&c.value);
                let expected = inf.annotated(&c.ty);
                inf.unify(value, expected, c.value.span);
            }
            Item::Function(f) if f.generics.is_empty() => inf.function(f),
            Item::Function(_) | Item::Struct(_) | Item::Enum(_) => {}
        }
    }

    if report_unknown {
        inf.finish();
    }
    let (call_targets, composite_calls) = inf.resolve_generic_calls();

    // Literals always have a default: their sets are subsets of NUMBER.
    let literal_types = inf
        .literals
        .iter()
        .filter_map(|&(item, span, var)| Some(((item, span), inf.u.set(var).default_type()?)))
        .collect();

    let expr_types = inf
        .expr_vars
        .iter()
        .filter_map(|&(item, span, var)| Some(((item, span), inf.u.resolve(var)?)))
        .collect();

    let operator_targets = inf
        .operator_calls
        .iter()
        .map(|&(item, span, name)| ((item, span), name.to_owned()))
        .collect();

    Inference {
        errors: inf.errors,
        literal_types,
        call_targets,
        operator_targets,
        composite_calls,
        expr_types,
    }
}

impl<'a> Infer<'a> {
    fn error(&mut self, message: String, span: Span) {
        if !self.reported.insert((span, message.clone())) {
            return;
        }
        // In a copy of a generic function, say which copy failed.
        let message = match self.current_fn {
            Some((name, _)) if name.contains('<') => format!("in `{name}`: {message}"),
            _ => message,
        };
        self.errors.push(CheckError::new(message, span));
    }

    fn known(&mut self, ty: Type) -> TyVar {
        self.u.new_var(TypeSet::single(ty))
    }

    /// The variable for a type in a signature or annotation. Generic names
    /// were replaced in the checked copies, and `resolve` rejected unknown
    /// names, so only structs are left here.
    fn annotated(&mut self, ty: &TypeExpr) -> TyVar {
        self.type_var(ty, &HashMap::new())
    }

    /// Like `annotated`, with the variables of the generic names in `ty`.
    fn type_var(&mut self, ty: &TypeExpr, generics: &HashMap<&str, TyVar>) -> TyVar {
        match ty {
            TypeExpr::Known(ty) => self.known(*ty),
            TypeExpr::Param(name) => match generics.get(name.name.as_str()) {
                Some(var) => *var,
                None => {
                    let id = self.struct_ids[name.name.as_str()];
                    self.u.new_struct(id)
                }
            },
            TypeExpr::Array(elem) => {
                let elem = self.type_var(elem, generics);
                self.u.new_array(elem)
            }
            TypeExpr::Optional(inner) => {
                let inner = self.type_var(inner, generics);
                self.u.new_optional(inner)
            }
            TypeExpr::Function { params, ret } => {
                let params = params.iter().map(|t| self.type_var(t, generics)).collect();
                let ret = ret.as_ref().map(|t| self.type_var(t, generics));
                self.u.new_function(params, ret)
            }
            TypeExpr::Applied { name, args } => {
                let id = self.struct_ids[name.name.as_str()];
                let args = args.iter().map(|t| self.type_var(t, generics)).collect();
                self.u.new_instance(id, args)
            }
            TypeExpr::Tuple(items) => {
                let elems = items.iter().map(|t| self.type_var(t, generics)).collect();
                self.u.new_tuple(elems)
            }
        }
    }

    /// A new value of the struct or enum `id`, with fresh type arguments,
    /// and what its generic names stand for.
    fn new_value(
        &mut self,
        id: usize,
        params: &'a [Ident],
        span: Span,
    ) -> (TyVar, HashMap<&'a str, TyVar>) {
        if params.is_empty() {
            return (self.u.new_struct(id), HashMap::new());
        }
        let args: Vec<TyVar> = params
            .iter()
            .map(|_| self.u.new_var(TypeSet::ANY))
            .collect();
        let value = self.u.new_instance(id, args.clone());
        self.generic_values.push((span, value));
        (value, arguments(params, &args))
    }

    /// `actual` must be `expected`. `span` is where `actual` comes from.
    fn expect(&mut self, actual: TyVar, expected: Type, span: Span) {
        let expected = self.known(expected);
        self.unify(actual, expected, span);
    }

    /// `actual` and `expected` must be the same type.
    fn unify(&mut self, actual: TyVar, expected: TyVar, span: Span) {
        if self.u.unify(actual, expected).is_err() {
            let (found, wanted) = (self.u.describe(actual), self.u.describe(expected));
            self.error(
                format!("mismatched types: expected {wanted}, found {found}"),
                span,
            );
        }
    }

    /// Narrows `var` to `allowed` for the operator `op`.
    fn restrict(&mut self, var: TyVar, allowed: TypeSet, op: &str, span: Span) {
        if self.u.restrict(var, allowed).is_err() {
            let found = self.u.describe(var);
            self.error(format!("cannot apply `{op}` to {found}"), span);
        }
    }

    // ----- items and statements -----

    fn function(&mut self, f: &'a Function) {
        let mut scope = HashMap::new();
        for param in &f.params {
            let var = self.annotated(&param.ty);
            scope.insert(param.name.name.as_str(), var);
        }
        self.scopes.push(scope);
        self.current_fn = Some((&f.name.name, f.ret.as_ref()));
        self.block(&f.body);
        self.current_fn = None;
        self.scopes.pop();
    }

    fn block(&mut self, block: &'a Block) {
        self.scopes.push(HashMap::new());
        for stmt in block {
            self.stmt(stmt);
        }
        self.scopes.pop();
    }

    fn lookup(&self, name: &str) -> Option<TyVar> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, ty, init } => {
                let var = match ty {
                    Some(ty) => self.annotated(ty),
                    None => self.u.new_var(TypeSet::ANY),
                };
                if let Some(init) = init {
                    let value = self.expr(init);
                    self.unify(value, var, init.span);
                }
                self.scopes
                    .last_mut()
                    .expect("inside a block")
                    .insert(&name.name, var);
                self.locals.push((name, var));
            }
            StmtKind::LetTuple { names, value } => self.let_tuple(names, value),
            StmtKind::Assign {
                name, op, value, ..
            } => {
                let value_ty = self.expr(value);
                // Constants and functions are rejected by `resolve`.
                if let Some(var) = self.lookup(&name.name) {
                    self.store(var, *op, value_ty, value.span, name.span, stmt.span);
                }
            }
            StmtKind::PathAssign {
                name,
                path,
                op,
                value,
                ..
            } => {
                let value_ty = self.expr(value);
                // Constants and functions are rejected by `resolve`.
                if let Some(mut var) = self.lookup(&name.name) {
                    for step in path {
                        var = match step {
                            PathStep::Index(index) => {
                                self.index_type(index);
                                self.array_elem(var, name.span)
                            }
                            PathStep::Cell(row, col) => {
                                self.index_type(row);
                                self.index_type(col);
                                self.cell_type(var, name.span)
                            }
                            PathStep::Field(field) => self.field_type(var, field, name.span),
                        };
                    }
                    self.store(var, *op, value_ty, value.span, name.span, stmt.span);
                }
            }
            StmtKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.condition(cond);
                self.block(then);
                if let Some(otherwise) = otherwise {
                    self.block(otherwise);
                }
            }
            StmtKind::While { cond, body } => {
                self.condition(cond);
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
                for bound in [start, end].into_iter().chain(step) {
                    let ty = self.expr(bound);
                    self.expect(ty, Type::Int, bound.span);
                }
                let var_ty = self.known(Type::Int);
                self.for_body(var, var_ty, body);
            }
            StmtKind::ForEach { var, array, body } => {
                let array_ty = self.expr(array);
                let var_ty = self.read_elem(array_ty, array.span);
                self.for_body(var, var_ty, body);
            }
            StmtKind::Match { value, arms } => self.match_stmt(value, arms),
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Return(value) => self.return_stmt(value.as_ref(), stmt.span),
            StmtKind::Expr(e) => match &e.kind {
                // A call to a function that returns nothing is fine here.
                ExprKind::Call { name, args } => {
                    self.call(name, args, e.span);
                }
                _ => {
                    self.expr(e);
                }
            },
        }
    }

    /// `resolve` already checked that every pattern names a real variant
    /// with the right number of bindings.
    fn match_stmt(&mut self, value: &'a Expr, arms: &'a [MatchArm]) {
        let ty = self.expr(value);
        let inner = self.u.optional_inner(ty);
        let matched = self
            .u
            .struct_id(ty)
            .and_then(|id| self.enum_defs.get(&id).copied());
        if inner.is_none() && matched.is_none() {
            let message = if self.u.set(ty) == TypeSet::ANY {
                "cannot tell the type of this value, add a type annotation".to_owned()
            } else {
                let found = self.u.describe(ty);
                format!("`match` needs an enum or an optional value, found {found}")
            };
            self.error(message, value.span);
        }

        let mut seen: HashSet<&str> = HashSet::new();
        let mut wildcard = false;
        for arm in arms {
            if wildcard {
                self.error(
                    "this arm comes after `_`, so it can never run".to_owned(),
                    arm.span,
                );
            }
            let vars = self.arm_bindings(arm, inner, matched, ty);
            match &arm.pattern {
                Pattern::Wildcard => wildcard = true,
                pattern => {
                    let name = match pattern {
                        Pattern::Variant { variant, .. } => variant.name.as_str(),
                        Pattern::Some(_) => "some",
                        _ => "none",
                    };
                    if !seen.insert(name) {
                        self.error(format!("`{name}` is already handled above"), arm.span);
                    }
                }
            }

            let scope = arm
                .pattern
                .bindings()
                .iter()
                .zip(vars)
                .filter(|(name, _)| name.name != "_")
                .map(|(name, var)| (name.name.as_str(), var))
                .collect();
            self.scopes.push(scope);
            self.block(&arm.body);
            self.scopes.pop();
        }

        if !wildcard {
            let missing: Vec<String> = match (inner, matched) {
                (Some(_), _) => ["some", "none"]
                    .into_iter()
                    .filter(|name| !seen.contains(name))
                    .map(|name| format!("`{name}`"))
                    .collect(),
                (None, Some(def)) => {
                    let short = def.name.name.rsplit('.').next().unwrap_or_default();
                    def.variants
                        .iter()
                        .filter(|v| !seen.contains(v.name.name.as_str()))
                        .map(|v| format!("`{short}::{}`", v.name.name))
                        .collect()
                }
                (None, None) => Vec::new(),
            };
            if !missing.is_empty() {
                self.error(
                    format!(
                        "this `match` does not handle {}; add it or `_`",
                        missing.join(", ")
                    ),
                    value.span,
                );
            }
        }
    }

    /// The types of the variables `arm` creates. Reports a pattern that does
    /// not fit the value.
    fn arm_bindings(
        &mut self,
        arm: &'a MatchArm,
        inner: Option<TyVar>,
        matched: Option<&'a EnumDef>,
        ty: TyVar,
    ) -> Vec<TyVar> {
        let fits = match &arm.pattern {
            Pattern::Wildcard => true,
            Pattern::Some(_) | Pattern::None => inner.is_some(),
            Pattern::Variant { name, .. } => matched.is_some_and(|def| def.name.name == name.name),
        };
        let known_kind = inner.is_some() || matched.is_some();
        if !fits && known_kind {
            let found = self.u.describe(ty);
            self.error(
                format!("this pattern does not fit a value of {found}"),
                arm.span,
            );
        }

        let declared = match (&arm.pattern, matched) {
            (Pattern::Variant { variant, .. }, Some(def)) if fits => def
                .variants
                .iter()
                .find(|v| v.name.name == variant.name)
                .map(|v| v.fields.as_slice()),
            _ => None,
        };
        match (&arm.pattern, declared, inner) {
            (_, Some(fields), _) => {
                let params = matched.map_or(&[][..], |def| def.generics.as_slice());
                let args = self.u.instance_args(ty);
                let names = arguments(params, &args);
                fields.iter().map(|f| self.type_var(f, &names)).collect()
            }
            (Pattern::Some(_), _, Some(inner)) => vec![inner],
            (pattern, _, _) => pattern
                .bindings()
                .iter()
                .map(|_| self.u.new_var(TypeSet::ANY))
                .collect(),
        }
    }

    /// `let (a, b) = value;`
    fn let_tuple(&mut self, names: &'a [Ident], value: &'a Expr) {
        let value_ty = self.expr(value);
        let vars: Vec<TyVar> = names.iter().map(|_| self.u.new_var(TypeSet::ANY)).collect();
        let tuple = self.u.new_tuple(vars.clone());
        if self.u.unify(value_ty, tuple).is_err() {
            let found = self.u.describe(value_ty);
            self.error(
                format!(
                    "expected a tuple of {} elements, found {found}",
                    names.len()
                ),
                value.span,
            );
        }

        for (name, var) in names.iter().zip(vars) {
            if name.name != "_" {
                self.scopes
                    .last_mut()
                    .expect("inside a block")
                    .insert(&name.name, var);
                self.locals.push((name, var));
            }
        }
    }

    /// `target = value`, or `target op= value` when `op` is set. The result
    /// must fit the target: `x *= v` is an error for `x: real`, `v: vec2`.
    fn store(
        &mut self,
        target: TyVar,
        op: Option<BinOp>,
        value: TyVar,
        value_span: Span,
        target_span: Span,
        span: Span,
    ) {
        let result = match op {
            Some(op) => self.binary_types(op, target, value, target_span, value_span, span),
            None => value,
        };
        self.unify(result, target, value_span);
    }

    fn for_body(&mut self, var: &'a Ident, ty: TyVar, body: &'a Block) {
        self.scopes.push(HashMap::from([(var.name.as_str(), ty)]));
        self.block(body);
        self.scopes.pop();
    }

    /// `return x;` must match the function's `-> type`; `return;` is only
    /// allowed in functions without one.
    fn return_stmt(&mut self, value: Option<&'a Expr>, span: Span) {
        let (name, ret) = self.current_fn.expect("return inside a function");
        match (value, ret) {
            (Some(value), Some(ret)) => {
                let ty = self.expr(value);
                let expected = self.annotated(ret);
                self.unify(ty, expected, value.span);
            }
            (Some(value), None) => {
                self.expr(value);
                self.error(
                    format!("`{name}` returns nothing, but `return` has a value"),
                    value.span,
                );
            }
            (None, Some(ret)) => {
                let ret = self.annotated(ret);
                let ret = self.u.describe(ret);
                self.error(format!("`{name}` must return a value of type {ret}"), span);
            }
            (None, None) => {}
        }
    }

    fn condition(&mut self, cond: &'a Expr) {
        let ty = self.expr(cond);
        self.expect(ty, Type::Bool, cond.span);
    }

    // ----- expressions -----

    fn expr(&mut self, e: &'a Expr) -> TyVar {
        let var = self.expr_kind(e);
        self.expr_vars.push((self.item, e.span, var));
        var
    }

    fn expr_kind(&mut self, e: &'a Expr) -> TyVar {
        match &e.kind {
            ExprKind::Int(_) => self.literal(TypeSet::NUMBER, e.span),
            ExprKind::Real(_) => self.literal(TypeSet::FLOAT, e.span),
            ExprKind::Complex { .. } => self.known(Type::Complex),
            ExprKind::Bool(_) => self.known(Type::Bool),
            ExprKind::Str(_) => self.known(Type::Str),
            ExprKind::Var(name) => match self.lookup(&name.name) {
                Some(var) => var,
                None => match self.consts.get(name.name.as_str()).copied() {
                    Some(ty) => self.annotated(ty),
                    None => match self.functions.get(name.name.as_str()).copied() {
                        Some(f) => self.function_value(f, e.span),
                        None => {
                            let builtin = kmpl_builtins::lookup_const(&name.name);
                            self.known(builtin.expect("checked by resolve").value.ty())
                        }
                    },
                },
            },
            ExprKind::Lambda(l) => self.lambda(l),
            ExprKind::Unary { op, operand } => {
                let ty = self.expr(operand);
                if *op == UnOp::Neg
                    && let Some(result) =
                        self.user_operator(Operator::Neg, &[(ty, operand.span)], e.span)
                {
                    return result;
                }
                match op {
                    UnOp::Neg => self.restrict_inner(ty, TypeSet::LINEAR, "-", e.span),
                    UnOp::Not => self.expect(ty, Type::Bool, operand.span),
                    UnOp::BitNot => self.expect(ty, Type::Int, operand.span),
                }
                ty
            }
            ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, e.span),
            ExprKind::Call { name, args } => match self.call(name, args, e.span) {
                Some(ty) => ty,
                None => {
                    self.error(format!("`{}` returns no value", name.name), e.span);
                    self.u.new_var(TypeSet::ANY)
                }
            },
            ExprKind::Swizzle { base, mask } => self.swizzle(base, mask),
            ExprKind::Field { base, name } => {
                let base_ty = self.expr(base);
                self.field_type(base_ty, name, base.span)
            }
            ExprKind::StructLit { name, fields } => self.struct_literal(name, fields, e.span),
            ExprKind::Variant {
                name,
                variant,
                args,
            } => self.variant_value(name, variant, args, e.span),
            ExprKind::Some(value) => {
                let inner = self.expr(value);
                self.u.new_optional(inner)
            }
            ExprKind::None => {
                let inner = self.u.new_var(TypeSet::ANY);
                let optional = self.u.new_optional(inner);
                self.nones.push((e.span, optional));
                optional
            }
            ExprKind::Tuple(items) => {
                let elems = items.iter().map(|item| self.expr(item)).collect();
                self.u.new_tuple(elems)
            }
            ExprKind::Array(items) => {
                let elem = self.u.new_var(TypeSet::ANY);
                for item in items {
                    let ty = self.expr(item);
                    self.unify(ty, elem, item.span);
                }
                let array = self.u.new_array(elem);
                self.array_literals.push((e.span, array));
                array
            }
            ExprKind::ArrayFill { value, len } => {
                let elem = self.expr(value);
                self.index_type(len);
                self.u.new_array(elem)
            }
            ExprKind::Index { base, index } => {
                let base_ty = self.expr(base);
                self.index_type(index);
                self.read_elem(base_ty, base.span)
            }
            ExprKind::Cell { base, row, col } => {
                let base_ty = self.expr(base);
                self.index_type(row);
                self.index_type(col);
                self.cell_type(base_ty, base.span)
            }
            ExprKind::Slice {
                base, start, end, ..
            } => {
                let base_ty = self.expr(base);
                for bound in [start, end].into_iter().flatten() {
                    self.index_type(bound);
                }
                self.read_elem(base_ty, base.span);
                base_ty
            }
        }
    }

    /// The type of field `field` of `ty`, which must be a known struct.
    fn field_type(&mut self, ty: TyVar, field: &Ident, base_span: Span) -> TyVar {
        let id = self.u.struct_id(ty);
        let Some(id) = id.filter(|id| self.struct_defs.contains_key(id)) else {
            let message = if self.u.set(ty) == TypeSet::ANY {
                "cannot tell the type of this value, add a type annotation".to_owned()
            } else {
                format!("only a struct has fields, found {}", self.u.describe(ty))
            };
            self.error(message, base_span);
            return self.u.new_var(TypeSet::ANY);
        };

        let def = self.struct_defs[&id];
        match def.fields.iter().find(|f| f.name.name == field.name) {
            Some(declared) => {
                let args = self.u.instance_args(ty);
                self.type_var(&declared.ty, &arguments(&def.generics, &args))
            }
            None => {
                self.error(
                    format!("`{}` has no field `{}`", def.name.name, field.name),
                    field.span,
                );
                self.u.new_var(TypeSet::ANY)
            }
        }
    }

    /// `resolve` already checked that the fields match the definition.
    fn struct_literal(
        &mut self,
        name: &'a Ident,
        fields: &'a [(Ident, Expr)],
        span: Span,
    ) -> TyVar {
        let id = self.struct_ids[name.name.as_str()];
        let def = self.struct_defs[&id];
        let (result, names) = self.new_value(id, &def.generics, span);
        for (field, value) in fields {
            let ty = self.expr(value);
            if let Some(declared) = def.fields.iter().find(|f| f.name.name == field.name) {
                let expected = self.type_var(&declared.ty, &names);
                self.unify(ty, expected, value.span);
            }
        }
        result
    }

    /// The type of a function used as a value, without calling it.
    fn function_value(&mut self, f: &'a Function, span: Span) -> TyVar {
        if !f.generics.is_empty() {
            self.error(
                format!(
                    "the generic function `{}` cannot be a value, wrap it in `fn(...) {{ ... }}`",
                    f.name.name
                ),
                span,
            );
            return self.u.new_var(TypeSet::ANY);
        }
        let params = f.params.iter().map(|p| self.annotated(&p.ty)).collect();
        let ret = f.ret.as_ref().map(|t| self.annotated(t));
        self.u.new_function(params, ret)
    }

    /// `fn(x: real) -> real { ... }`. The body sees the variables around it.
    fn lambda(&mut self, l: &'a Lambda) -> TyVar {
        let mut scope = HashMap::new();
        let mut params = Vec::with_capacity(l.params.len());
        for param in &l.params {
            let var = self.annotated(&param.ty);
            scope.insert(param.name.name.as_str(), var);
            params.push(var);
        }

        let outer = self.current_fn.replace(("function", l.ret.as_ref()));
        self.scopes.push(scope);
        self.block(&l.body);
        self.scopes.pop();
        self.current_fn = outer;

        let ret = l.ret.as_ref().map(|t| self.annotated(t));
        self.u.new_function(params, ret)
    }

    /// A call through a variable that holds a function.
    fn call_value(
        &mut self,
        name: &Ident,
        params: Vec<TyVar>,
        ret: Option<TyVar>,
        args: &'a [Expr],
    ) -> Option<TyVar> {
        if params.len() != args.len() {
            self.error(
                format!(
                    "`{}` takes {} arguments, but {} were given",
                    name.name,
                    params.len(),
                    args.len()
                ),
                name.span,
            );
        }
        for (arg, param) in args.iter().zip(params) {
            let ty = self.expr(arg);
            self.unify(ty, param, arg.span);
        }
        ret
    }

    /// `resolve` already checked the variant and the number of arguments.
    fn variant_value(
        &mut self,
        name: &'a Ident,
        variant: &Ident,
        args: &'a [Expr],
        span: Span,
    ) -> TyVar {
        let id = self.struct_ids[name.name.as_str()];
        let def = self.enum_defs[&id];
        let (result, names) = self.new_value(id, &def.generics, span);
        let declared = def
            .variants
            .iter()
            .find(|v| v.name.name == variant.name)
            .expect("checked by resolve");
        for (arg, field) in args.iter().zip(&declared.fields) {
            let ty = self.expr(arg);
            let expected = self.type_var(field, &names);
            self.unify(ty, expected, arg.span);
        }
        result
    }

    /// An index or an array length is an `int`.
    fn index_type(&mut self, e: &'a Expr) {
        let ty = self.expr(e);
        self.expect(ty, Type::Int, e.span);
    }

    /// What reading from `ty` gives. Like `array_elem`, and a string can be
    /// read too, but not written to: a character is a string of one.
    fn read_elem(&mut self, ty: TyVar, span: Span) -> TyVar {
        if self.u.set(ty) == TypeSet::single(Type::Str) {
            return self.known(Type::Str);
        }
        self.array_elem(ty, span)
    }

    /// The element type of `ty`, which must be an array.
    fn array_elem(&mut self, ty: TyVar, span: Span) -> TyVar {
        // Bytes read and write as `int`s from 0 to 255.
        if self.is_bytes(ty) {
            return self.known(Type::Int);
        }
        if self.is_matrix(ty) {
            self.error(
                "a `matrix` takes a row and a column: `m[i, j]`".to_owned(),
                span,
            );
            return self.u.new_var(TypeSet::ANY);
        }
        let elem = self.u.new_var(TypeSet::ANY);
        let array = self.u.new_array(elem);
        if self.u.unify(ty, array).is_err() {
            let found = self.u.describe(ty);
            self.error(format!("expected an array, found {found}"), span);
        }
        elem
    }

    /// What `ty[row, col]` gives: `ty` must be a `matrix`.
    fn cell_type(&mut self, ty: TyVar, span: Span) -> TyVar {
        if self.u.element(ty).is_some() {
            let found = self.u.describe(ty);
            self.error(
                format!("only a `matrix` takes a row and a column, found {found}: write `a[i][j]`"),
                span,
            );
        } else {
            self.expect(ty, Type::Matrix, span);
        }
        self.known(Type::Real)
    }

    fn is_matrix(&self, ty: TyVar) -> bool {
        self.u.set(ty) == TypeSet::MATRIX
    }

    fn is_bytes(&self, ty: TyVar) -> bool {
        self.u.set(ty) == TypeSet::single(Type::Bytes)
    }

    fn literal(&mut self, set: TypeSet, span: Span) -> TyVar {
        let var = self.u.new_var(set);
        self.literals.push((self.item, span, var));
        var
    }

    fn binary(&mut self, op: BinOp, lhs: &'a Expr, rhs: &'a Expr, span: Span) -> TyVar {
        let l = self.expr(lhs);
        let r = self.expr(rhs);
        self.binary_types(op, l, r, lhs.span, rhs.span, span)
    }

    fn binary_types(
        &mut self,
        op: BinOp,
        l: TyVar,
        r: TyVar,
        lhs_span: Span,
        rhs_span: Span,
        span: Span,
    ) -> TyVar {
        let sym = op_symbol(op);
        let operands = [(l, lhs_span), (r, rhs_span)];
        if let Some(result) = self.user_operator(Operator::Binary(op), &operands, span) {
            return result;
        }

        match op {
            BinOp::Add | BinOp::Sub => {
                self.unify(r, l, rhs_span);
                let allowed = match op {
                    BinOp::Add => TypeSet::ADDABLE,
                    _ => TypeSet::LINEAR,
                };
                self.restrict_inner(l, allowed, sym, span);
                l
            }
            BinOp::Mul | BinOp::Div => self.mul_div(op, l, r, rhs_span, span),
            BinOp::Rem => {
                self.unify(r, l, rhs_span);
                self.restrict(l, TypeSet::ORDERED, sym, span);
                l
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                self.unify(r, l, rhs_span);
                self.restrict(l, TypeSet::COMPARABLE, sym, span);
                self.known(Type::Bool)
            }
            BinOp::Eq | BinOp::Ne => {
                self.unify(r, l, rhs_span);
                if self.u.function_parts(l).is_some() {
                    self.error(format!("cannot apply `{sym}` to functions"), span);
                }
                self.known(Type::Bool)
            }
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                self.expect(l, Type::Int, lhs_span);
                self.expect(r, Type::Int, rhs_span);
                self.known(Type::Int)
            }
            BinOp::And | BinOp::Or => {
                self.expect(l, Type::Bool, lhs_span);
                self.expect(r, Type::Bool, rhs_span);
                self.known(Type::Bool)
            }
        }
    }

    /// The operators a program can define have no built-in meaning for a
    /// struct or an enum. So when an operand is one, the answer is here: the
    /// type of the value of the definition that fits, or an error. `span` is
    /// what the use is recorded under.
    fn user_operator(
        &mut self,
        operator: Operator,
        operands: &[(TyVar, Span)],
        span: Span,
    ) -> Option<TyVar> {
        let can_be_defined = match operator {
            Operator::Neg => true,
            Operator::Binary(op) => matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Rem
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
            ),
        };
        let on_own_type = operands
            .iter()
            .any(|(operand, _)| self.u.struct_id(*operand).is_some());
        if !can_be_defined || !on_own_type {
            return None;
        }

        let mut fitting = self.fitting_operators(operator, operands, false);
        if fitting.len() > 1 {
            // `2 * v` with definitions for `int` and for `real`: the literal
            // is an `int` unless something says otherwise, as everywhere.
            fitting = self.fitting_operators(operator, operands, true);
        }

        let types: Vec<String> = operands
            .iter()
            .map(|(operand, _)| self.u.describe(*operand))
            .collect();
        let types = types.join(" and ");
        match fitting.as_slice() {
            [f] => {
                for (param, (operand, operand_span)) in f.params.iter().zip(operands) {
                    let expected = self.annotated(&param.ty);
                    self.unify(*operand, expected, *operand_span);
                }
                self.operator_calls
                    .push((self.item, span, f.name.name.as_str()));
                let ret = f.ret.as_ref().expect("the parser requires a type");
                return Some(self.annotated(ret));
            }
            [] => self.error(
                format!("cannot apply `{}` to {types}", operator.symbol()),
                span,
            ),
            _ => self.error(
                format!(
                    "several definitions of operator `{}` fit {types}, add a type annotation",
                    operator.symbol()
                ),
                span,
            ),
        }
        Some(self.u.new_var(TypeSet::ANY))
    }

    /// Gives every number still open in `var` its default type, also inside
    /// `Pair<...>` and arrays: `Pair { a: 3, b: 4 }` is a `Pair<int>`.
    fn settle_literals(&mut self, var: TyVar) {
        if let Some(ty) = self.u.set(var).default_type() {
            let _ = self.u.restrict(var, TypeSet::single(ty));
        }
        for inner in self.u.instance_args(var) {
            self.settle_literals(inner);
        }
        if let Some(element) = self.u.element(var) {
            self.settle_literals(element);
        }
    }

    /// The definitions of `operator` whose parameters can be these operands.
    /// With `defaults`, a number literal counts as the type it would get if
    /// nothing else decided.
    fn fitting_operators(
        &mut self,
        operator: Operator,
        operands: &[(TyVar, Span)],
        defaults: bool,
    ) -> Vec<&'a Function> {
        let defined: Vec<&'a Function> = self
            .operators
            .iter()
            .copied()
            .filter(|f| f.operator == Some(operator))
            .collect();

        let mut fitting = Vec::new();
        for f in defined {
            // Tried on a copy: a definition that does not fit must leave no
            // trace in the types.
            let saved = self.u.clone();
            let fits = f.params.iter().zip(operands).all(|(param, (operand, _))| {
                if defaults {
                    self.settle_literals(*operand);
                }
                let expected = self.annotated(&param.ty);
                self.u.unify(*operand, expected).is_ok()
            });
            self.u = saved;
            if fits {
                fitting.push(f);
            }
        }
        fitting
    }

    /// `+`, `-` and unary `-` work on the types in `allowed` and on arrays
    /// of them, at any depth.
    fn restrict_inner(&mut self, var: TyVar, allowed: TypeSet, op: &str, span: Span) {
        let mut inner = var;
        while let Some(elem) = self.u.element(inner) {
            inner = elem;
        }
        if self.u.restrict(inner, allowed).is_err() {
            let found = self.u.describe(var);
            self.error(format!("cannot apply `{op}` to {found}"), span);
        }
    }

    /// An array with a non-array applies the operator to every element.
    /// Two arrays can only be a matrix product.
    fn mul_div(&mut self, op: BinOp, l: TyVar, r: TyVar, rhs_span: Span, span: Span) -> TyVar {
        match (self.u.element(l), self.u.element(r)) {
            (None, None) => self.scalar_mul_div(op, l, r, rhs_span, span),
            // `matrix * [real]` is a `[real]`, the other way round is nothing.
            (None, Some(elem)) if op == BinOp::Mul && self.is_matrix(l) => {
                self.expect(elem, Type::Real, rhs_span);
                r
            }
            (Some(_), None) if self.is_matrix(r) => self.bad_operands(op, l, r, span),
            (Some(elem), None) => {
                let result = self.mul_div(op, elem, r, rhs_span, span);
                self.u.new_array(result)
            }
            (None, Some(elem)) if op == BinOp::Mul => {
                let result = self.mul_div(op, l, elem, rhs_span, span);
                self.u.new_array(result)
            }
            (Some(row), Some(r_elem)) if op == BinOp::Mul => {
                self.matrix_mul(l, r, row, r_elem, rhs_span, span)
            }
            _ => self.bad_operands(op, l, r, span),
        }
    }

    /// `[[T]] * [[T]]` is `[[T]]` and `[[T]] * [T]` is `[T]`, for a number
    /// `T`. `row` and `r_elem` are the element types of `l` and `r`.
    fn matrix_mul(
        &mut self,
        l: TyVar,
        r: TyVar,
        row: TyVar,
        r_elem: TyVar,
        rhs_span: Span,
        span: Span,
    ) -> TyVar {
        let (r_number, result) = match self.u.element(r_elem) {
            Some(number) => (number, l),
            None => (r_elem, row),
        };

        match self.u.element(row) {
            Some(number) if self.u.element(number).is_none() => {
                self.unify(r_number, number, rhs_span);
                self.restrict(number, TypeSet::NUMBER, "*", span);
                result
            }
            _ => self.bad_operands(BinOp::Mul, l, r, span),
        }
    }

    fn bad_operands(&mut self, op: BinOp, l: TyVar, r: TyVar, span: Span) -> TyVar {
        let sym = op_symbol(op);
        let (l, r) = (self.u.describe(l), self.u.describe(r));
        self.error(format!("cannot apply `{sym}` to {l} and {r}"), span);
        self.u.new_var(TypeSet::ANY)
    }

    /// `number * number`, `vec * real`, `real * vec`, `number / number`,
    /// `vec / real`. A `matrix` scales like a vector, and two multiply.
    fn scalar_mul_div(
        &mut self,
        op: BinOp,
        l: TyVar,
        r: TyVar,
        rhs_span: Span,
        span: Span,
    ) -> TyVar {
        let sym = op_symbol(op);
        let (ls, rs) = (self.u.set(l), self.u.set(r));
        let is_number = |s: TypeSet| s.is_subset(TypeSet::NUMBER);
        // Scaled by a `real`, like a vector.
        let is_vector = |s: TypeSet| s.is_subset(TypeSet::VECTOR) || s == TypeSet::MATRIX;

        if is_number(ls) && is_number(rs) {
            self.unify(r, l, rhs_span);
            return l;
        }
        if op == BinOp::Mul && ls == TypeSet::MATRIX && rs == TypeSet::MATRIX {
            return l;
        }
        if is_vector(ls) && is_number(rs) {
            self.expect(r, Type::Real, rhs_span);
            return l;
        }
        if op == BinOp::Mul && is_number(ls) && is_vector(rs) {
            self.expect(l, Type::Real, span);
            return r;
        }

        // A side is "unclear" if it may be a number or a vector, e.g. a
        // value of `print`'s `Any` parameter. Then an annotation helps;
        // otherwise the types are simply wrong (`bool`, `vec * vec`, ...).
        let linear = |s: TypeSet| !s.intersect(TypeSet::LINEAR).is_empty();
        let unclear = |s: TypeSet| linear(s) && !is_number(s) && !is_vector(s);
        if (unclear(ls) && linear(rs)) || (unclear(rs) && linear(ls)) {
            self.error(
                format!(
                    "cannot tell if the operands of `{sym}` are numbers or vectors, add a type annotation"
                ),
                span,
            );
            return self.u.new_var(TypeSet::ANY);
        }
        self.bad_operands(op, l, r, span)
    }

    /// Checks a call and returns the type of its value, or `None` if the
    /// function returns nothing.
    fn call(&mut self, name: &'a Ident, args: &'a [Expr], span: Span) -> Option<TyVar> {
        let held = match self.lookup(&name.name) {
            Some(var) => Some(var),
            None => self
                .consts
                .get(name.name.as_str())
                .copied()
                .map(|ty| self.annotated(ty)),
        };
        if let Some(var) = held
            && let Some((params, ret)) = self.u.function_parts(var)
        {
            return self.call_value(name, params, ret, args);
        }

        if let Some(&f) = self.functions.get(name.name.as_str()) {
            return self.call_user(f, args, span);
        }

        let Some(builtin) = kmpl_builtins::lookup(&name.name) else {
            self.error(format!("`{}` is not a function", name.name), name.span);
            return Some(self.u.new_var(TypeSet::ANY));
        };
        let t = self.u.new_var(TypeSet::of(builtin.generic));
        let elem = self.u.new_var(TypeSet::ANY);
        // `push` and `concat` also work on `bytes` and then give `bytes`.
        let mut on_bytes = false;
        // `length` of a vector is a `real`.
        let mut on_vector = false;

        for (arg, param) in args.iter().zip(builtin.params) {
            let arg_ty = self.expr(arg);
            match param {
                ParamType::Exact(ty) => self.expect(arg_ty, *ty, arg.span),
                ParamType::T => self.unify(arg_ty, t, arg.span),
                ParamType::Any => {}
                ParamType::Collection => {
                    let string = self.u.set(arg_ty) == TypeSet::single(Type::Str);
                    on_vector = self.u.set(arg_ty).is_subset(TypeSet::VECTOR);
                    if !string && !on_vector && !self.is_bytes(arg_ty) {
                        self.array_elem(arg_ty, arg.span);
                    }
                }
                ParamType::Elem => self.unify(arg_ty, elem, arg.span),
                ParamType::OptionalOfElem => {
                    let optional = self.u.new_optional(elem);
                    self.unify(arg_ty, optional, arg.span);
                }
                ParamType::Reals => {
                    let expected = self.reals();
                    self.unify(arg_ty, expected, arg.span);
                }
                ParamType::Rows => {
                    let expected = self.rows();
                    self.unify(arg_ty, expected, arg.span);
                }
                ParamType::ArrayOfElem if on_bytes || self.is_bytes(arg_ty) => {
                    on_bytes = true;
                    self.expect(elem, Type::Int, arg.span);
                    self.expect(arg_ty, Type::Bytes, arg.span);
                }
                ParamType::ArrayOfElem => {
                    let array = self.u.new_array(elem);
                    self.unify(arg_ty, array, arg.span);
                }
            }
        }

        match builtin.ret {
            RetType::Nothing => None,
            RetType::Exact(ty) => Some(self.known(ty)),
            RetType::T => Some(t),
            RetType::ArrayOfElem if on_bytes => Some(self.known(Type::Bytes)),
            RetType::ArrayOfElem => Some(self.u.new_array(elem)),
            RetType::ArrayOf(ty) => {
                let elem = self.known(ty);
                Some(self.u.new_array(elem))
            }
            RetType::Length if on_vector => Some(self.known(Type::Real)),
            RetType::Length => Some(self.known(Type::Int)),
            RetType::Elem => Some(elem),
            RetType::Rows => Some(self.rows()),
            RetType::OptionalReals => {
                let inner = self.reals();
                Some(self.u.new_optional(inner))
            }
            RetType::Optional(ty) => {
                let inner = self.known(ty);
                Some(self.u.new_optional(inner))
            }
            RetType::Pair(a, b) => {
                let elems = vec![self.known(a), self.known(b)];
                Some(self.u.new_tuple(elems))
            }
            RetType::Magnitude => {
                if self.u.set(t) == TypeSet::single(Type::Complex) {
                    Some(self.known(Type::Real))
                } else {
                    // Still open, e.g. `abs(-1)`: the result type would
                    // depend on the choice, so drop `complex`. It is the
                    // last default anyway.
                    let _ = self.u.restrict(t, TypeSet::ORDERED);
                    Some(t)
                }
            }
        }
    }

    /// `[real]`
    fn reals(&mut self) -> TyVar {
        let element = self.known(Type::Real);
        self.u.new_array(element)
    }

    /// `[[real]]`
    fn rows(&mut self) -> TyVar {
        let row = self.reals();
        self.u.new_array(row)
    }

    /// A call to a user function. For a generic one, each parameter gets a
    /// fresh variable narrowed to its bound; the call is resolved to a copy
    /// once inference is done.
    fn call_user(&mut self, f: &'a Function, args: &'a [Expr], span: Span) -> Option<TyVar> {
        let mut vars = HashMap::new();
        for generic in &f.generics {
            // Without a bound it can be anything, an array or a struct too.
            let set = match generic.bound.len() == Type::ALL.len() {
                true => TypeSet::ANY,
                false => TypeSet::of(&generic.bound),
            };
            let var = self.u.new_var(set);
            vars.insert(generic.name.name.as_str(), var);
        }

        for (arg, param) in args.iter().zip(&f.params) {
            let arg_ty = self.expr(arg);
            let param_ty = self.type_var(&param.ty, &vars);
            self.unify(arg_ty, param_ty, arg.span);
        }
        let ret = f.ret.as_ref().map(|ty| self.type_var(ty, &vars));

        if !f.generics.is_empty() {
            let vars = f
                .generics
                .iter()
                .map(|g| vars[g.name.name.as_str()])
                .collect();
            self.generic_calls.push(GenericCall {
                item: self.item,
                span,
                name: &f.name.name,
                vars,
            });
        }
        ret
    }

    fn swizzle(&mut self, base: &'a Expr, mask: &Ident) -> TyVar {
        let base_ty = self.expr(base);
        let set = self.u.set(base_ty);

        let width = match set.as_single() {
            Some(Type::Vec2) => 2,
            Some(Type::Vec3) => 3,
            Some(Type::Vec4) => 4,
            _ => {
                let message = if set.is_subset(TypeSet::VECTOR) {
                    "cannot tell which vector this is, add a type annotation".to_owned()
                } else {
                    format!("swizzle needs a vector, found {}", self.u.describe(base_ty))
                };
                self.error(message, base.span);
                return self.u.new_var(TypeSet::ANY);
            }
        };

        let allowed = &"xyzw"[..width];
        let len = mask.name.len();
        let valid = (1..=4).contains(&len) && mask.name.chars().all(|c| allowed.contains(c));
        if !valid {
            self.error(
                format!(
                    "invalid swizzle `{}` for `vec{width}`: use 1 to 4 of the letters `{allowed}`",
                    mask.name
                ),
                mask.span,
            );
            return self.u.new_var(TypeSet::ANY);
        }

        let ty = match len {
            1 => Type::Real,
            2 => Type::Vec2,
            3 => Type::Vec3,
            _ => Type::Vec4,
        };
        self.known(ty)
    }

    // ----- end of inference -----

    /// Every `let` variable must end up with a type. Skipped when there
    /// are other type errors, which usually cause these.
    fn finish(&mut self) {
        if !self.errors.is_empty() {
            return;
        }
        for (name, var) in std::mem::take(&mut self.locals) {
            if !self.u.is_resolved(var) {
                let set = self.u.describe(var);
                let message = if self.u.set(var) == TypeSet::ANY {
                    format!(
                        "cannot infer the type of `{}`, add a type annotation",
                        name.name
                    )
                } else if [TypeSet::ARRAY, TypeSet::TUPLE, TypeSet::OPTIONAL]
                    .contains(&self.u.set(var))
                {
                    format!(
                        "cannot infer the element type of `{}`, add a type annotation",
                        name.name
                    )
                } else {
                    format!(
                        "cannot infer the type of `{}`: it may be {set}, add a type annotation",
                        name.name
                    )
                };
                self.error(message, name.span);
            }
        }

        if !self.errors.is_empty() {
            return;
        }
        for (span, var) in std::mem::take(&mut self.array_literals) {
            if !self.u.is_resolved(var) {
                self.error(
                    "cannot infer the element type of this array".to_owned(),
                    span,
                );
            }
        }

        for (span, var) in std::mem::take(&mut self.generic_values) {
            if !self.u.is_resolved(var) {
                self.error(
                    "cannot infer the type arguments of this value, add a type annotation"
                        .to_owned(),
                    span,
                );
            }
        }

        for (span, var) in std::mem::take(&mut self.nones) {
            if !self.u.is_resolved(var) {
                self.error(
                    "cannot infer the type of this `none`, add a type annotation".to_owned(),
                    span,
                );
            }
        }
    }

    /// The copy each generic call uses, from the final types of its
    /// parameters.
    #[allow(clippy::type_complexity)]
    fn resolve_generic_calls(
        &mut self,
    ) -> (HashMap<(usize, Span), String>, Vec<(String, Vec<Ty>)>) {
        let mut targets = HashMap::new();
        let mut composite = Vec::new();
        for call in std::mem::take(&mut self.generic_calls) {
            let types: Option<Vec<Ty>> = call.vars.iter().map(|&var| self.u.resolve(var)).collect();
            match types {
                Some(types) => {
                    let written: Vec<TypeExpr> =
                        types.iter().map(|ty| type_expr(ty, call.span)).collect();
                    targets.insert((call.item, call.span), instance_name(call.name, &written));
                    if types.iter().any(|ty| !matches!(ty, Ty::Prim(_))) {
                        composite.push((call.name.to_owned(), types));
                    }
                }
                // Skipped after other errors, which usually cause this one.
                None if !self.errors.is_empty() => {}
                None => {
                    self.current_fn = None;
                    self.error(
                        format!(
                            "cannot infer the generic parameters of `{}` here, add a type annotation",
                            call.name
                        ),
                        call.span,
                    );
                }
            }
        }
        (targets, composite)
    }
}

/// What the generic names of a struct or enum stand for.
fn arguments<'a>(params: &'a [Ident], args: &[TyVar]) -> HashMap<&'a str, TyVar> {
    params
        .iter()
        .map(|p| p.name.as_str())
        .zip(args.iter().copied())
        .collect()
}

fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}
