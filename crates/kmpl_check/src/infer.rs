//! Type inference by unification. See `types.rs` for type variables.
//!
//! Each expression gets a type variable. Operators, calls, `let` and
//! assignments add constraints by unifying variables or narrowing their
//! sets. After the whole program is walked, variables that still have
//! several candidates get a default (`int`, then `real`, then `complex`).
//!
//! `*`, `/` and swizzles must know whether an operand is a number or a
//! vector when they are reached. In practice it always is, because a
//! variable cannot be read before it is assigned.
//!
//! Runs only on programs without name errors, so every name exists and
//! every call has the right number of arguments. Generic functions are
//! skipped: their monomorphized copies (`generics.rs`) are checked instead,
//! and a call to a generic function picks its copy from the inferred types.

use std::collections::{HashMap, HashSet};

use kmpl_ast::{
    BinOp, Block, Expr, ExprKind, Function, Ident, Item, Program, Span, Stmt, StmtKind, Type,
    TypeExpr, UnOp,
};
use kmpl_builtins::{ParamType, RetType};

use crate::error::CheckError;
use crate::generics::instance_name;
use crate::types::{TyVar, TypeSet, Unifier};

/// A call to a generic function, resolved to a copy at the end.
struct GenericCall<'a> {
    item: usize,
    span: Span,
    name: &'a str,
    /// One variable per generic parameter, in declaration order.
    vars: Vec<TyVar>,
}

/// A type in a signature or annotation. Generic parameters were replaced
/// in the checked copies, and `resolve` rejected unknown names.
fn known(ty: &TypeExpr) -> Type {
    ty.known()
        .expect("generic names are replaced before inference")
}

struct Infer<'a> {
    u: Unifier,
    functions: HashMap<&'a str, &'a Function>,
    consts: HashMap<&'a str, Type>,
    /// Name -> type of a local. Innermost scope is last.
    scopes: Vec<HashMap<&'a str, TyVar>>,
    /// Every `let` variable, to check at the end that its type is known.
    locals: Vec<(&'a Ident, TyVar)>,
    /// Every `1` and `1.0` literal with its item index, to pick its final
    /// type at the end. Copies of a generic function share spans, so the
    /// span alone is not unique.
    literals: Vec<(usize, Span, TyVar)>,
    generic_calls: Vec<GenericCall<'a>>,
    /// Index of the item being checked.
    item: usize,
    /// Errors already reported, so one mistake in a generic body is not
    /// repeated for every copy.
    reported: HashSet<(Span, String)>,
    /// The function being checked: its name and return type.
    current_fn: Option<(&'a str, Option<Type>)>,
    errors: Vec<CheckError>,
}

pub struct Inference {
    pub errors: Vec<CheckError>,
    /// Final type of every integer and real literal, by item index and span.
    pub literal_types: HashMap<(usize, Span), Type>,
    /// The copy each call to a generic function uses, like `lerp<vec2>`,
    /// by item index and span of the call.
    pub call_targets: HashMap<(usize, Span), String>,
}

/// `report_unknown`: report `let` variables whose type stays unknown.
/// Off when earlier checks already failed, since their errors (e.g. a
/// variable never assigned) usually cause these.
pub fn infer(program: &Program, report_unknown: bool) -> Inference {
    let mut inf = Infer {
        u: Unifier::default(),
        functions: HashMap::new(),
        consts: HashMap::new(),
        scopes: Vec::new(),
        locals: Vec::new(),
        current_fn: None,
        literals: Vec::new(),
        generic_calls: Vec::new(),
        item: 0,
        reported: HashSet::new(),
        errors: Vec::new(),
    };

    for item in &program.items {
        match item {
            Item::Const(c) => {
                inf.consts.insert(&c.name.name, known(&c.ty));
            }
            Item::Function(f) => {
                inf.functions.insert(&f.name.name, f);
            }
        }
    }

    for (index, item) in program.items.iter().enumerate() {
        inf.item = index;
        match item {
            Item::Const(c) => {
                let value = inf.expr(&c.value);
                inf.expect(value, known(&c.ty), c.value.span);
            }
            Item::Function(f) if f.generics.is_empty() => inf.function(f),
            Item::Function(_) => {}
        }
    }

    if report_unknown {
        inf.finish();
    }
    let call_targets = inf.resolve_generic_calls();

    // Literals always have a default: their sets are subsets of NUMBER.
    let literal_types = inf
        .literals
        .iter()
        .filter_map(|&(item, span, var)| Some(((item, span), inf.u.set(var).default_type()?)))
        .collect();

    Inference {
        errors: inf.errors,
        literal_types,
        call_targets,
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

    /// `actual` must be `expected`. `span` is where `actual` comes from.
    fn expect(&mut self, actual: TyVar, expected: Type, span: Span) {
        let expected = self.known(expected);
        self.unify(actual, expected, span);
    }

    /// `actual` and `expected` must be the same type.
    fn unify(&mut self, actual: TyVar, expected: TyVar, span: Span) {
        if let Err((found, wanted)) = self.u.unify(actual, expected) {
            self.error(
                format!("mismatched types: expected {wanted}, found {found}"),
                span,
            );
        }
    }

    /// Narrows `var` to `allowed` for the operator `op`.
    fn restrict(&mut self, var: TyVar, allowed: TypeSet, op: &str, span: Span) {
        if let Err(found) = self.u.restrict(var, allowed) {
            self.error(format!("cannot apply `{op}` to {found}"), span);
        }
    }

    // ----- items and statements -----

    fn function(&mut self, f: &'a Function) {
        let mut scope = HashMap::new();
        for param in &f.params {
            let var = self.known(known(&param.ty));
            scope.insert(param.name.name.as_str(), var);
        }
        self.scopes.push(scope);
        self.current_fn = Some((&f.name.name, f.ret.as_ref().map(known)));
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
                    Some(ty) => self.known(known(ty)),
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
            StmtKind::Assign { name, value } => {
                let value_ty = self.expr(value);
                // Constants and functions are rejected by `resolve`.
                if let Some(var) = self.lookup(&name.name) {
                    self.unify(value_ty, var, value.span);
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

    /// `return x;` must match the function's `-> type`; `return;` is only
    /// allowed in functions without one.
    fn return_stmt(&mut self, value: Option<&'a Expr>, span: Span) {
        let (name, ret) = self.current_fn.expect("return inside a function");
        match (value, ret) {
            (Some(value), Some(ret)) => {
                let ty = self.expr(value);
                self.expect(ty, ret, value.span);
            }
            (Some(value), None) => {
                self.expr(value);
                self.error(
                    format!("`{name}` returns nothing, but `return` has a value"),
                    value.span,
                );
            }
            (None, Some(ret)) => {
                let ret = TypeSet::single(ret);
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
        match &e.kind {
            ExprKind::Int(_) => self.literal(TypeSet::NUMBER, e.span),
            ExprKind::Real(_) => self.literal(TypeSet::FLOAT, e.span),
            ExprKind::Complex { .. } => self.known(Type::Complex),
            ExprKind::Bool(_) => self.known(Type::Bool),
            ExprKind::Var(name) => match self.lookup(&name.name) {
                Some(var) => var,
                None => {
                    let ty = match self.consts.get(name.name.as_str()) {
                        Some(ty) => *ty,
                        None => {
                            let builtin = kmpl_builtins::lookup_const(&name.name);
                            builtin.expect("checked by resolve").value.ty()
                        }
                    };
                    self.known(ty)
                }
            },
            ExprKind::Unary { op, operand } => {
                let ty = self.expr(operand);
                match op {
                    UnOp::Neg => self.restrict(ty, TypeSet::LINEAR, "-", e.span),
                    UnOp::Not => self.expect(ty, Type::Bool, operand.span),
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
        }
    }

    fn literal(&mut self, set: TypeSet, span: Span) -> TyVar {
        let var = self.u.new_var(set);
        self.literals.push((self.item, span, var));
        var
    }

    fn binary(&mut self, op: BinOp, lhs: &'a Expr, rhs: &'a Expr, span: Span) -> TyVar {
        let l = self.expr(lhs);
        let r = self.expr(rhs);
        let sym = op_symbol(op);

        match op {
            BinOp::Add | BinOp::Sub => {
                self.unify(r, l, rhs.span);
                self.restrict(l, TypeSet::LINEAR, sym, span);
                l
            }
            BinOp::Mul | BinOp::Div => self.mul_div(op, l, r, rhs.span, span),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                self.unify(r, l, rhs.span);
                self.restrict(l, TypeSet::ORDERED, sym, span);
                self.known(Type::Bool)
            }
            BinOp::Eq | BinOp::Ne => {
                self.unify(r, l, rhs.span);
                self.known(Type::Bool)
            }
            BinOp::And | BinOp::Or => {
                self.expect(l, Type::Bool, lhs.span);
                self.expect(r, Type::Bool, rhs.span);
                self.known(Type::Bool)
            }
        }
    }

    /// `number * number`, `vec * real`, `real * vec`, `number / number`,
    /// `vec / real`.
    fn mul_div(&mut self, op: BinOp, l: TyVar, r: TyVar, rhs_span: Span, span: Span) -> TyVar {
        let sym = op_symbol(op);
        let (ls, rs) = (self.u.set(l), self.u.set(r));
        let is_number = |s: TypeSet| s.is_subset(TypeSet::NUMBER);
        let is_vector = |s: TypeSet| s.is_subset(TypeSet::VECTOR);

        if is_number(ls) && is_number(rs) {
            self.unify(r, l, rhs_span);
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
        let message = if (unclear(ls) && linear(rs)) || (unclear(rs) && linear(ls)) {
            format!(
                "cannot tell if the operands of `{sym}` are numbers or vectors, add a type annotation"
            )
        } else {
            format!("cannot apply `{sym}` to {ls} and {rs}")
        };
        self.error(message, span);
        self.u.new_var(TypeSet::ANY)
    }

    /// Checks a call and returns the type of its value, or `None` if the
    /// function returns nothing.
    fn call(&mut self, name: &'a Ident, args: &'a [Expr], span: Span) -> Option<TyVar> {
        if let Some(&f) = self.functions.get(name.name.as_str()) {
            return self.call_user(f, args, span);
        }

        let builtin = kmpl_builtins::lookup(&name.name).expect("checked by resolve");
        let t = self.u.new_var(TypeSet::of(builtin.generic));

        for (arg, param) in args.iter().zip(builtin.params) {
            let arg_ty = self.expr(arg);
            match param {
                ParamType::Exact(ty) => self.expect(arg_ty, *ty, arg.span),
                ParamType::T => self.unify(arg_ty, t, arg.span),
                ParamType::Any => {}
            }
        }

        match builtin.ret {
            RetType::Nothing => None,
            RetType::Exact(ty) => Some(self.known(ty)),
            RetType::T => Some(t),
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

    /// A call to a user function. For a generic one, each parameter gets a
    /// fresh variable narrowed to its bound; the call is resolved to a copy
    /// once inference is done.
    fn call_user(&mut self, f: &'a Function, args: &'a [Expr], span: Span) -> Option<TyVar> {
        let mut vars = HashMap::new();
        for generic in &f.generics {
            let var = self.u.new_var(TypeSet::of(&generic.bound));
            vars.insert(generic.name.name.as_str(), var);
        }
        let var_of = |inf: &mut Self, ty: &TypeExpr| match ty {
            TypeExpr::Known(ty) => inf.known(*ty),
            TypeExpr::Param(name) => vars[name.name.as_str()],
        };

        for (arg, param) in args.iter().zip(&f.params) {
            let arg_ty = self.expr(arg);
            let param_ty = var_of(self, &param.ty);
            self.unify(arg_ty, param_ty, arg.span);
        }
        let ret = f.ret.as_ref().map(|ty| var_of(self, ty));

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
                    format!("swizzle needs a vector, found {set}")
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
            let set = self.u.set(var);
            if set.default_type().is_none() {
                let message = if set == TypeSet::ANY {
                    format!(
                        "cannot infer the type of `{}`, add a type annotation",
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
    }

    /// The copy each generic call uses, from the final types of its
    /// parameters.
    fn resolve_generic_calls(&mut self) -> HashMap<(usize, Span), String> {
        let mut targets = HashMap::new();
        for call in std::mem::take(&mut self.generic_calls) {
            let types: Option<Vec<Type>> = call
                .vars
                .iter()
                .map(|&var| self.u.set(var).default_type())
                .collect();
            match types {
                Some(types) => {
                    targets.insert((call.item, call.span), instance_name(call.name, &types));
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
        targets
    }
}

fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
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
