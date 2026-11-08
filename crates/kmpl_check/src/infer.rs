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

struct Infer<'a> {
    u: Unifier,
    functions: HashMap<&'a str, &'a Function>,
    consts: HashMap<&'a str, &'a TypeExpr>,
    /// Name -> type of a local. Innermost scope is last.
    scopes: Vec<HashMap<&'a str, TyVar>>,
    /// Every `let` variable, to check at the end that its type is known.
    locals: Vec<(&'a Ident, TyVar)>,
    /// Every array literal, to check at the end that its element type is
    /// known: `print([]);` has no variable to report.
    array_literals: Vec<(Span, TyVar)>,
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
        array_literals: Vec::new(),
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
                inf.consts.insert(&c.name.name, &c.ty);
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
                let expected = inf.annotated(&c.ty);
                inf.unify(value, expected, c.value.span);
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

    /// The variable for a type in a signature or annotation. Generic names
    /// were replaced in the checked copies, and `resolve` rejected unknown
    /// names, so none are left here.
    fn annotated(&mut self, ty: &TypeExpr) -> TyVar {
        self.type_var(ty, &HashMap::new())
    }

    /// Like `annotated`, with the variables of the generic names in `ty`.
    fn type_var(&mut self, ty: &TypeExpr, generics: &HashMap<&str, TyVar>) -> TyVar {
        match ty {
            TypeExpr::Known(ty) => self.known(*ty),
            TypeExpr::Param(name) => generics[name.name.as_str()],
            TypeExpr::Array(elem) => {
                let elem = self.type_var(elem, generics);
                self.u.new_array(elem)
            }
        }
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
            StmtKind::Assign { name, value } => {
                let value_ty = self.expr(value);
                // Constants and functions are rejected by `resolve`.
                if let Some(var) = self.lookup(&name.name) {
                    self.unify(value_ty, var, value.span);
                }
            }
            StmtKind::IndexAssign {
                name,
                indices,
                value,
            } => {
                let value_ty = self.expr(value);
                // Constants and functions are rejected by `resolve`.
                if let Some(mut var) = self.lookup(&name.name) {
                    for index in indices {
                        self.index_type(index);
                        var = self.array_elem(var, name.span);
                    }
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
            StmtKind::For {
                var,
                start,
                end,
                body,
            } => {
                for bound in [start, end] {
                    let ty = self.expr(bound);
                    self.expect(ty, Type::Int, bound.span);
                }
                let var_ty = self.known(Type::Int);
                self.for_body(var, var_ty, body);
            }
            StmtKind::ForEach { var, array, body } => {
                let array_ty = self.expr(array);
                let var_ty = self.array_elem(array_ty, array.span);
                self.for_body(var, var_ty, body);
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
        match &e.kind {
            ExprKind::Int(_) => self.literal(TypeSet::NUMBER, e.span),
            ExprKind::Real(_) => self.literal(TypeSet::FLOAT, e.span),
            ExprKind::Complex { .. } => self.known(Type::Complex),
            ExprKind::Bool(_) => self.known(Type::Bool),
            ExprKind::Var(name) => match self.lookup(&name.name) {
                Some(var) => var,
                None => match self.consts.get(name.name.as_str()).copied() {
                    Some(ty) => self.annotated(ty),
                    None => {
                        let builtin = kmpl_builtins::lookup_const(&name.name);
                        self.known(builtin.expect("checked by resolve").value.ty())
                    }
                },
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
                self.array_elem(base_ty, base.span)
            }
        }
    }

    /// An index or an array length is an `int`.
    fn index_type(&mut self, e: &'a Expr) {
        let ty = self.expr(e);
        self.expect(ty, Type::Int, e.span);
    }

    /// The element type of `ty`, which must be an array.
    fn array_elem(&mut self, ty: TyVar, span: Span) -> TyVar {
        let elem = self.u.new_var(TypeSet::ANY);
        let array = self.u.new_array(elem);
        if self.u.unify(ty, array).is_err() {
            let found = self.u.describe(ty);
            self.error(format!("expected an array, found {found}"), span);
        }
        elem
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
            let (l, r) = (self.u.describe(l), self.u.describe(r));
            format!("cannot apply `{sym}` to {l} and {r}")
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
                ParamType::AnyArray => {
                    self.array_elem(arg_ty, arg.span);
                }
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
                } else if self.u.set(var) == TypeSet::ARRAY {
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
