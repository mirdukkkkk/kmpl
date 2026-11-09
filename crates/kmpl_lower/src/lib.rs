//! Translates a checked program into the IR.
//!
//! Local variables become SSA registers without any analysis of the whole
//! function, because control flow is structured. A variable has a current
//! register, and an assignment just changes which one that is. Where paths
//! meet, variables whose registers differ become block parameters; a loop
//! header gets one for every variable its body assigns.
//!
//! A variable with no register yet (`let x;`) needs no parameter: the
//! checker guarantees nobody reads it until it is assigned on every path.

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use kmpl_ast::{
    self as ast, BinOp, Expr, ExprKind, Ident, Item, MatchArm, PathStep, Pattern, Program, Span,
    Stmt, StmtKind, Ty, Type, TypeExpr, UnOp,
};
use kmpl_builtins::Value;
use kmpl_check::TypeInfo;
use kmpl_ir::{
    Block, BlockId, Constant, FuncId, Function, Inst, Module, Op, Reg, Step, StructLayout, Target,
    Terminator,
};

const INT: Ty = Ty::Prim(Type::Int);
const BOOL: Ty = Ty::Prim(Type::Bool);

/// One function a value can hold.
///
/// A function value is a value of a hidden enum with one variant per
/// candidate, carrying what the function captured. A call through a variable
/// tests which variant it holds, among the functions with that signature,
/// and calls that function directly. All functions are known at compile
/// time, so nothing new is needed in the back end.
struct Candidate {
    func: FuncId,
    /// A function without a name gets the value itself as its first
    /// argument, to read what it captured.
    takes_itself: bool,
    signature: Ty,
}

struct Pending<'p> {
    func: FuncId,
    lambda: &'p ast::Lambda,
    item: usize,
    captured: Vec<(&'p str, Ty)>,
    signature: Ty,
}

/// What every function being lowered needs to know about the program.
struct Context<'p> {
    types: &'p TypeInfo,
    functions: HashMap<&'p str, FuncId>,
    globals: HashMap<&'p str, usize>,
    /// Name of a struct or enum -> index in `layouts` and `field_types`.
    structs: HashMap<&'p str, usize>,
    layouts: Vec<StructLayout>,
    /// Empty for an enum.
    field_types: Vec<Vec<Ty>>,
    /// Enum name -> what each of its variants carries.
    variant_types: HashMap<&'p str, Vec<Vec<Ty>>>,
    /// Name of a struct or enum -> its generic names, which appear in
    /// `field_types` and `variant_types` until they are replaced.
    type_params: HashMap<&'p str, Vec<String>>,
    /// The types of the constants, by name.
    const_types: HashMap<&'p str, Ty>,
    /// Everything a function value can be, in the whole program.
    candidates: Vec<Candidate>,
    /// A named function used as a value -> its index in `candidates`.
    named: HashMap<&'p str, usize>,
    /// A function without a name, by item and span -> index in `candidates`.
    lambdas: HashMap<(usize, Span), usize>,
    /// Index in `layouts` of the layout function values use.
    function_layout: usize,
    /// Functions without a name found while lowering, waiting for their turn.
    pending: Vec<Pending<'p>>,
    builtins: Vec<String>,
}

impl Context<'_> {
    fn builtin_id(&mut self, name: &str) -> usize {
        match self.builtins.iter().position(|b| b == name) {
            Some(id) => id,
            None => {
                self.builtins.push(name.to_owned());
                self.builtins.len() - 1
            }
        }
    }
}

fn to_ty(ty: &TypeExpr) -> Ty {
    match ty {
        TypeExpr::Known(ty) => Ty::Prim(*ty),
        // Generic parameters are gone by now, so this is a struct or enum.
        TypeExpr::Param(name) => Ty::Struct(name.name.clone()),
        TypeExpr::Array(elem) => Ty::Array(Box::new(to_ty(elem))),
        TypeExpr::Optional(inner) => Ty::Optional(Box::new(to_ty(inner))),
        TypeExpr::Function { params, ret } => Ty::Function {
            params: params.iter().map(to_ty).collect(),
            ret: ret.as_ref().map(|t| Box::new(to_ty(t))),
        },
        TypeExpr::Applied { name, args } => Ty::Instance {
            name: name.name.clone(),
            args: args.iter().map(to_ty).collect(),
        },
        TypeExpr::Tuple(items) => Ty::Tuple(items.iter().map(to_ty).collect()),
    }
}

/// The name and type arguments of a struct or enum type.
fn nominal(ty: &Ty) -> Option<(&str, &[Ty])> {
    match ty {
        Ty::Struct(name) => Some((name, &[])),
        Ty::Instance { name, args } => Some((name, args)),
        _ => None,
    }
}

/// `ty` from the definition of a generic type, with its names replaced.
fn substitute(ty: &Ty, params: &[String], args: &[Ty]) -> Ty {
    let each = |items: &[Ty]| items.iter().map(|t| substitute(t, params, args)).collect();
    match ty {
        Ty::Struct(name) => match params.iter().position(|p| p == name) {
            Some(index) => args[index].clone(),
            None => ty.clone(),
        },
        Ty::Prim(_) => ty.clone(),
        Ty::Array(element) => Ty::Array(Box::new(substitute(element, params, args))),
        Ty::Optional(inner) => Ty::Optional(Box::new(substitute(inner, params, args))),
        Ty::Tuple(items) => Ty::Tuple(each(items)),
        Ty::Instance { name, args: inner } => Ty::Instance {
            name: name.clone(),
            args: each(inner),
        },
        Ty::Function { params: inner, ret } => Ty::Function {
            params: each(inner),
            ret: ret.as_ref().map(|t| Box::new(substitute(t, params, args))),
        },
    }
}

fn signature_of(params: &[ast::Param], ret: Option<&TypeExpr>) -> Ty {
    Ty::Function {
        params: params.iter().map(|p| to_ty(&p.ty)).collect(),
        ret: ret.map(|t| Box::new(to_ty(t))),
    }
}

/// Calls `f` with every expression in `block`, also inside functions without
/// a name.
fn visit_block<'p>(block: &'p ast::Block, f: &mut impl FnMut(&'p Expr)) {
    for stmt in block {
        match &stmt.kind {
            StmtKind::Let { init, .. } => {
                if let Some(init) = init {
                    visit_expr(init, f);
                }
            }
            StmtKind::LetTuple { value, .. } | StmtKind::Assign { value, .. } => {
                visit_expr(value, f);
            }
            StmtKind::PathAssign { path, value, .. } => {
                for step in path {
                    for index in step.indices() {
                        visit_expr(index, f);
                    }
                }
                visit_expr(value, f);
            }
            StmtKind::If {
                cond,
                then,
                otherwise,
            } => {
                visit_expr(cond, f);
                visit_block(then, f);
                if let Some(otherwise) = otherwise {
                    visit_block(otherwise, f);
                }
            }
            StmtKind::While { cond, body } => {
                visit_expr(cond, f);
                visit_block(body, f);
            }
            StmtKind::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                visit_expr(start, f);
                visit_expr(end, f);
                if let Some(step) = step {
                    visit_expr(step, f);
                }
                visit_block(body, f);
            }
            StmtKind::ForEach { array, body, .. } => {
                visit_expr(array, f);
                visit_block(body, f);
            }
            StmtKind::Match { value, arms } => {
                visit_expr(value, f);
                for arm in arms {
                    visit_block(&arm.body, f);
                }
            }
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    visit_expr(value, f);
                }
            }
            StmtKind::Expr(e) => visit_expr(e, f),
            StmtKind::Break | StmtKind::Continue => {}
        }
    }
}

fn visit_expr<'p>(e: &'p Expr, f: &mut impl FnMut(&'p Expr)) {
    f(e);
    match &e.kind {
        ExprKind::Int(_)
        | ExprKind::Real(_)
        | ExprKind::Complex { .. }
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Var(_)
        | ExprKind::None => {}
        ExprKind::Unary { operand, .. } => visit_expr(operand, f),
        ExprKind::Binary { lhs, rhs, .. } => {
            visit_expr(lhs, f);
            visit_expr(rhs, f);
        }
        ExprKind::Call { args, .. }
        | ExprKind::Variant { args, .. }
        | ExprKind::Tuple(args)
        | ExprKind::Array(args) => {
            for arg in args {
                visit_expr(arg, f);
            }
        }
        ExprKind::Swizzle { base, .. } | ExprKind::Field { base, .. } | ExprKind::Some(base) => {
            visit_expr(base, f);
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, value) in fields {
                visit_expr(value, f);
            }
        }
        ExprKind::ArrayFill { value, len } => {
            visit_expr(value, f);
            visit_expr(len, f);
        }
        ExprKind::Index { base, index } => {
            visit_expr(base, f);
            visit_expr(index, f);
        }
        ExprKind::Cell { base, row, col } => {
            visit_expr(base, f);
            visit_expr(row, f);
            visit_expr(col, f);
        }
        ExprKind::Slice {
            base, start, end, ..
        } => {
            visit_expr(base, f);
            for bound in [start, end].into_iter().flatten() {
                visit_expr(bound, f);
            }
        }
        ExprKind::Lambda(l) => visit_block(&l.body, f),
    }
}

/// Finds every function used as a value. A name that is really a local
/// variable only adds a candidate nobody chooses.
fn collect_candidates<'p>(ctx: &mut Context<'p>, program: &'p Program, first_lambda: u32) {
    let mut next = first_lambda;
    let definitions: HashMap<&str, &ast::Function> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some((f.name.name.as_str(), f)),
            _ => None,
        })
        .collect();

    for (index, item) in program.items.iter().enumerate() {
        let mut visit = |e: &'p Expr| match &e.kind {
            ExprKind::Var(name) => {
                if let Some(definition) = definitions.get(name.name.as_str())
                    && !ctx.named.contains_key(name.name.as_str())
                {
                    ctx.named.insert(&name.name, ctx.candidates.len());
                    ctx.candidates.push(Candidate {
                        func: ctx.functions[name.name.as_str()],
                        takes_itself: false,
                        signature: signature_of(&definition.params, definition.ret.as_ref()),
                    });
                }
            }
            ExprKind::Lambda(l) => {
                ctx.lambdas.insert((index, e.span), ctx.candidates.len());
                ctx.candidates.push(Candidate {
                    func: FuncId(next),
                    takes_itself: true,
                    signature: signature_of(&l.params, l.ret.as_ref()),
                });
                next += 1;
            }
            _ => {}
        };
        match item {
            Item::Const(c) => visit_expr(&c.value, &mut visit),
            Item::Function(f) => visit_block(&f.body, &mut visit),
            Item::Struct(_) | Item::Enum(_) => {}
        }
    }
}

/// A function without a name: the value itself comes first, and what it
/// captured is read from that.
fn lower_lambda<'p>(ctx: &mut Context<'p>, pending: Pending<'p>) -> Function {
    let Pending {
        lambda,
        item,
        captured,
        signature,
        ..
    } = pending;
    let mut b = Builder::new(ctx, item, "<fn>", lambda.ret.as_ref().map(to_ty));

    let itself = b.new_reg(signature);
    let mut params = vec![itself];
    let mut named = Vec::with_capacity(lambda.params.len());
    for param in &lambda.params {
        let reg = b.new_reg(to_ty(&param.ty));
        params.push(reg);
        named.push((param.name.name.as_str(), reg));
    }
    b.func.blocks[0].params.clone_from(&params);
    b.func.params = params;

    let span = lambda.body.first().map_or(Span::new(0, 0), |s| s.span);
    for (index, (name, ty)) in captured.into_iter().enumerate() {
        let field = Op::VariantField {
            base: itself,
            index,
        };
        let value = b.value(field, ty, span);
        b.declare(name, Some(value));
    }
    for (name, reg) in named {
        b.declare(name, Some(reg));
    }

    b.block(&lambda.body);
    b.finish()
}

/// A function that is never created, because the code making it can never
/// run. It only has to have the right parameters.
fn unused_lambda<'p>(ctx: &mut Context<'p>, candidate: usize) -> Function {
    let signature = ctx.candidates[candidate].signature.clone();
    let Ty::Function { params, ret } = signature.clone() else {
        panic!("a candidate has a function type");
    };
    let mut b = Builder::new(ctx, 0, "<fn>", ret.map(|t| *t));
    let mut registers = vec![b.new_reg(signature)];
    for ty in params {
        registers.push(b.new_reg(ty));
    }
    b.func.blocks[0].params.clone_from(&registers);
    b.func.params = registers;
    b.terminate(Terminator::Unreachable);
    b.func
}

/// The result still needs `kmpl_ir::annotate` before it can run.
pub fn lower(program: &Program, types: &TypeInfo) -> Module {
    let mut ctx = Context {
        types,
        functions: HashMap::new(),
        globals: HashMap::new(),
        structs: HashMap::new(),
        layouts: Vec::new(),
        field_types: Vec::new(),
        variant_types: HashMap::new(),
        type_params: HashMap::new(),
        const_types: HashMap::new(),
        candidates: Vec::new(),
        named: HashMap::new(),
        lambdas: HashMap::new(),
        function_layout: 0,
        pending: Vec::new(),
        builtins: Vec::new(),
    };

    for item in &program.items {
        match item {
            Item::Function(f) => {
                let id = FuncId(ctx.functions.len() as u32);
                ctx.functions.insert(&f.name.name, id);
            }
            Item::Const(c) => {
                let id = ctx.globals.len();
                ctx.globals.insert(&c.name.name, id);
                ctx.const_types.insert(&c.name.name, to_ty(&c.ty));
            }
            Item::Struct(s) => {
                let params = s.generics.iter().map(|g| g.name.clone()).collect();
                ctx.type_params.insert(&s.name.name, params);
                ctx.structs.insert(&s.name.name, ctx.layouts.len());
                ctx.layouts.push(StructLayout {
                    name: s.name.name.clone(),
                    fields: s.fields.iter().map(|f| f.name.name.clone()).collect(),
                });
                ctx.field_types
                    .push(s.fields.iter().map(|f| to_ty(&f.ty)).collect());
            }
            Item::Enum(e) => {
                let params = e.generics.iter().map(|g| g.name.clone()).collect();
                ctx.type_params.insert(&e.name.name, params);
                ctx.structs.insert(&e.name.name, ctx.layouts.len());
                ctx.layouts.push(StructLayout {
                    name: e.name.name.clone(),
                    fields: e.variants.iter().map(|v| v.name.name.clone()).collect(),
                });
                ctx.field_types.push(Vec::new());
                let carried = e
                    .variants
                    .iter()
                    .map(|v| v.fields.iter().map(to_ty).collect())
                    .collect();
                ctx.variant_types.insert(&e.name.name, carried);
            }
        }
    }

    ctx.function_layout = ctx.layouts.len();
    ctx.layouts.push(StructLayout {
        name: "<fn>".to_owned(),
        fields: Vec::new(),
    });
    // After the ordinary functions and the one computing the constants.
    let first_lambda = ctx.functions.len() as u32 + 1;
    collect_candidates(&mut ctx, program, first_lambda);

    let mut functions = Vec::new();
    for (index, item) in program.items.iter().enumerate() {
        if let Item::Function(f) = item {
            functions.push(lower_function(&mut ctx, index, f));
        }
    }
    let init = FuncId(functions.len() as u32);
    functions.push(lower_constants(&mut ctx, program));

    // Lowering one of them can find more.
    let mut lambdas = HashMap::new();
    while let Some(pending) = ctx.pending.pop() {
        let id = pending.func;
        lambdas.insert(id.0, lower_lambda(&mut ctx, pending));
    }
    for candidate in 0..ctx.candidates.len() {
        if ctx.candidates[candidate].takes_itself {
            let id = ctx.candidates[candidate].func.0;
            functions.push(match lambdas.remove(&id) {
                Some(function) => function,
                None => unused_lambda(&mut ctx, candidate),
            });
        }
    }

    let main = ctx.functions["main"];
    Module {
        functions,
        init,
        main,
        globals: ctx.globals.len(),
        builtins: ctx.builtins,
        structs: ctx.layouts,
    }
}

fn lower_function<'p>(ctx: &mut Context<'p>, item: usize, f: &'p ast::Function) -> Function {
    let mut b = Builder::new(ctx, item, &f.name.name, f.ret.as_ref().map(to_ty));

    let mut params = Vec::with_capacity(f.params.len());
    for param in &f.params {
        let reg = b.new_reg(to_ty(&param.ty));
        params.push(reg);
        b.declare(&param.name.name, Some(reg));
    }
    b.func.blocks[0].params.clone_from(&params);
    b.func.params = params;

    b.block(&f.body);
    b.finish()
}

/// The function that computes every `const`, in order.
fn lower_constants<'p>(ctx: &mut Context<'p>, program: &'p Program) -> Function {
    let mut b = Builder::new(ctx, 0, "__init", None);
    for (index, item) in program.items.iter().enumerate() {
        if let Item::Const(c) = item {
            b.item = index;
            let value = b.expr(&c.value);
            let id = b.ctx.globals[c.name.name.as_str()];
            b.effect(Op::SetGlobal(id, value), c.value.span);
        }
    }
    b.finish()
}

/// The numbers a `for` loop counts through.
struct Range {
    start: Reg,
    end: Reg,
    step: Reg,
    inclusive: bool,
}

/// A loop being lowered, for `break` and `continue`.
struct Loop {
    /// Where `continue` goes.
    next: BlockId,
    exit: BlockId,
    /// The variables passed to `next` and `exit`.
    assigned: Vec<usize>,
}

struct Builder<'c, 'p> {
    ctx: &'c mut Context<'p>,
    /// Index of the item being lowered, to look types up.
    item: usize,
    func: Function,
    current: BlockId,
    /// The current block already ended, in `return`, `break` or `continue`.
    terminated: bool,
    /// The current register of every variable in scope.
    vars: Vec<Option<Reg>>,
    /// Name -> index in `vars`. Innermost scope is last.
    scopes: Vec<HashMap<&'p str, usize>>,
    loops: Vec<Loop>,
}

impl<'c, 'p> Builder<'c, 'p> {
    fn new(ctx: &'c mut Context<'p>, item: usize, name: &str, ret: Option<Ty>) -> Self {
        let mut builder = Builder {
            ctx,
            item,
            func: Function {
                name: name.to_owned(),
                params: Vec::new(),
                ret,
                blocks: Vec::new(),
                types: Vec::new(),
            },
            current: BlockId(0),
            terminated: false,
            vars: Vec::new(),
            scopes: vec![HashMap::new()],
            loops: Vec::new(),
        };
        builder.new_block(Vec::new());
        builder
    }

    fn finish(mut self) -> Function {
        if !self.terminated {
            self.terminate(Terminator::Return(None));
        }
        self.func
    }

    // ----- building blocks -----

    fn new_reg(&mut self, ty: Ty) -> Reg {
        self.func.types.push(ty);
        Reg(self.func.types.len() as u32 - 1)
    }

    fn reg_type(&self, reg: Reg) -> Ty {
        self.func.types[reg.0 as usize].clone()
    }

    fn new_block(&mut self, params: Vec<Reg>) -> BlockId {
        self.func.blocks.push(Block {
            params,
            insts: Vec::new(),
            term: Terminator::Unreachable,
            term_frees: Vec::new(),
        });
        BlockId(self.func.blocks.len() as u32 - 1)
    }

    fn switch_to(&mut self, block: BlockId) {
        self.current = block;
        self.terminated = false;
    }

    fn terminate(&mut self, term: Terminator) {
        self.func.blocks[self.current.0 as usize].term = term;
        self.terminated = true;
    }

    fn jump(&mut self, block: BlockId, args: Vec<Reg>) {
        self.terminate(Terminator::Jump(Target { block, args }));
    }

    fn push(&mut self, dest: Option<Reg>, op: Op, span: Span) {
        self.func.blocks[self.current.0 as usize].insts.push(Inst {
            dest,
            op,
            span,
            frees: Vec::new(),
        });
    }

    fn value(&mut self, op: Op, ty: Ty, span: Span) -> Reg {
        let dest = self.new_reg(ty);
        self.push(Some(dest), op, span);
        dest
    }

    fn effect(&mut self, op: Op, span: Span) {
        self.push(None, op, span);
    }

    // ----- variables -----

    fn declare(&mut self, name: &'p str, value: Option<Reg>) {
        self.vars.push(value);
        let id = self.vars.len() - 1;
        self.scopes
            .last_mut()
            .expect("inside a function")
            .insert(name, id);
    }

    fn lookup(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn register_of(&self, var: usize) -> Reg {
        self.vars[var].expect("checker ensures assignment before use")
    }

    /// The current registers of `vars`.
    fn registers_of(&self, vars: &[usize]) -> Vec<Reg> {
        vars.iter().map(|&v| self.register_of(v)).collect()
    }

    /// New registers with the types of `vars`, to use as block parameters.
    fn params_for(&mut self, vars: &[usize]) -> Vec<Reg> {
        vars.iter()
            .map(|&v| {
                let ty = self.reg_type(self.register_of(v));
                self.new_reg(ty)
            })
            .collect()
    }

    fn bind(&mut self, vars: &[usize], registers: &[Reg]) {
        for (&var, &reg) in vars.iter().zip(registers) {
            self.vars[var] = Some(reg);
        }
    }

    /// The variables in scope that `body` may assign and that have a value.
    /// A shadowing `let` can make this list too long, which is harmless.
    fn assigned_in(&self, body: &'p ast::Block) -> Vec<usize> {
        let mut names = HashSet::new();
        collect_assigned(body, &mut names);

        let mut vars: Vec<usize> = names
            .iter()
            .filter_map(|name| self.lookup(name))
            .filter(|&var| self.vars[var].is_some())
            .collect();
        vars.sort_unstable();
        vars
    }

    // ----- statements -----

    fn block(&mut self, block: &'p ast::Block) {
        self.scopes.push(HashMap::new());
        let base = self.vars.len();
        for stmt in block {
            if self.terminated {
                break;
            }
            self.stmt(stmt);
        }
        self.scopes.pop();
        self.vars.truncate(base);
    }

    fn stmt(&mut self, stmt: &'p Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, init, .. } => {
                let value = init.as_ref().map(|init| self.expr(init));
                self.declare(&name.name, value);
            }
            StmtKind::LetTuple { names, value } => self.let_tuple(names, value),
            StmtKind::Assign {
                name,
                op,
                via,
                value,
            } => {
                let value = self.expr(value);
                let var = self.lookup(&name.name).expect("checked by resolve");
                let result = match op {
                    Some(op) => {
                        let old = self.register_of(var);
                        let ty = self.reg_type(old);
                        let op = self.operation(*op, via.as_ref(), old, value);
                        self.value(op, ty, stmt.span)
                    }
                    None => value,
                };
                self.vars[var] = Some(result);
            }
            StmtKind::PathAssign {
                name,
                path,
                op,
                via,
                value,
            } => self.path_assign(name, path, *op, via.as_ref(), value, stmt.span),
            StmtKind::If {
                cond,
                then,
                otherwise,
            } => self.if_stmt(cond, then, otherwise.as_ref()),
            StmtKind::While { cond, body } => self.while_stmt(cond, body),
            StmtKind::For {
                var,
                start,
                end,
                inclusive,
                step,
                body,
            } => {
                let start = self.expr(start);
                let end = self.expr(end);
                let by = match step {
                    Some(step) => {
                        let by = self.expr(step);
                        self.effect(Op::CheckStep(by), step.span);
                        by
                    }
                    None => self.value(Op::Const(Constant::Int(1)), INT, stmt.span),
                };
                let range = Range {
                    start,
                    end,
                    step: by,
                    inclusive: *inclusive,
                };
                self.for_loop(var, body, range, None, stmt.span);
            }
            StmtKind::ForEach { var, array, body } => {
                let mut elements = self.expr(array);
                // Once through the text, instead of from its start each time.
                if self.reg_type(elements) == Ty::Prim(Type::Str) {
                    let id = self.ctx.builtin_id("chars");
                    let args = vec![elements];
                    let ty = Ty::Array(Box::new(Ty::Prim(Type::Str)));
                    elements = self.value(Op::Builtin { id, args }, ty, array.span);
                }
                let zero = self.value(Op::Const(Constant::Int(0)), INT, stmt.span);
                let one = self.value(Op::Const(Constant::Int(1)), INT, stmt.span);
                let len = self.value(Op::ArrayLen(elements), INT, stmt.span);
                let range = Range {
                    start: zero,
                    end: len,
                    step: one,
                    inclusive: false,
                };
                self.for_loop(var, body, range, Some(elements), stmt.span);
            }
            StmtKind::Match { value, arms } => {
                let subject = self.expr(value);
                let tag = self.value(Op::VariantOf(subject), INT, value.span);
                self.match_arms(subject, tag, arms, value.span);
            }
            StmtKind::Break | StmtKind::Continue => {
                let target = self.loops.last().expect("checked by resolve");
                let block = match stmt.kind {
                    StmtKind::Break => target.exit,
                    _ => target.next,
                };
                let assigned = target.assigned.clone();
                let args = self.registers_of(&assigned);
                self.jump(block, args);
            }
            StmtKind::Return(value) => {
                let value = value.as_ref().map(|value| self.expr(value));
                self.terminate(Terminator::Return(value));
            }
            StmtKind::Expr(e) => match &e.kind {
                // May be a call to a function that returns nothing.
                ExprKind::Call { name, args } => {
                    self.call(name, args, None, e.span);
                    if stmt.never_returns() {
                        self.terminate(Terminator::Unreachable);
                    }
                }
                _ => {
                    self.expr(e);
                }
            },
        }
    }

    fn let_tuple(&mut self, names: &'p [Ident], value: &'p Expr) {
        let tuple = self.expr(value);
        let Ty::Tuple(types) = self.reg_type(tuple) else {
            panic!("unpacking a non-tuple, the checker should have rejected it");
        };
        for (index, (name, ty)) in names.iter().zip(types).enumerate() {
            if name.name != "_" {
                let element = self.value(Op::TupleGet { tuple, index }, ty, value.span);
                self.declare(&name.name, Some(element));
            }
        }
    }

    /// `a[i].x = value` or `a[i].x += value`. The value runs first, then
    /// the indices, then the old value is read.
    fn path_assign(
        &mut self,
        name: &Ident,
        path: &'p [PathStep],
        op: Option<BinOp>,
        via: Option<&Ident>,
        value: &'p Expr,
        span: Span,
    ) {
        let mut value = self.expr(value);
        let var = self.lookup(&name.name).expect("checked by resolve");
        let base = self.register_of(var);

        let mut ty = self.reg_type(base);
        let mut steps = Vec::with_capacity(path.len());
        for step in path {
            match step {
                PathStep::Index(index) => {
                    steps.push(Step::Index(self.expr(index), index.span));
                    ty = element_type(ty);
                }
                PathStep::Cell(row, col) => {
                    steps.push(Step::Index(self.expr(row), row.span));
                    steps.push(Step::Index(self.expr(col), col.span));
                    ty = Ty::Prim(Type::Real);
                }
                PathStep::Field(field) => {
                    let (index, field_ty) = self.field(&ty, &field.name);
                    steps.push(Step::Field(index));
                    ty = field_ty;
                }
            }
        }

        if let Some(op) = op {
            let old = self.value(
                Op::PathGet {
                    base,
                    path: steps.clone(),
                },
                ty.clone(),
                span,
            );
            let op = self.operation(op, via, old, value);
            value = self.value(op, ty, span);
        }

        let base_ty = self.reg_type(base);
        let result = self.value(
            Op::PathSet {
                base,
                path: steps,
                value,
                take: false,
            },
            base_ty,
            span,
        );
        self.vars[var] = Some(result);
    }

    /// `a op b` in `x op= b`: the built-in operator, or a call to the
    /// function behind an operator the program defines.
    fn operation(&self, op: BinOp, via: Option<&Ident>, a: Reg, b: Reg) -> Op {
        match via {
            Some(function) => Op::Call {
                func: self.ctx.functions[function.name.as_str()],
                args: vec![a, b],
            },
            None => Op::Binary { op, a, b },
        }
    }

    /// Position and type of a field of the struct type `ty`.
    fn field(&self, ty: &Ty, name: &str) -> (usize, Ty) {
        let Some((struct_name, args)) = nominal(ty) else {
            panic!("a field of a non-struct, the checker should have rejected it");
        };
        let id = self.ctx.structs[struct_name];
        let index = self.ctx.layouts[id]
            .fields
            .iter()
            .position(|f| f == name)
            .expect("checked by inference");
        let params = &self.ctx.type_params[struct_name];
        (
            index,
            substitute(&self.ctx.field_types[id][index], params, args),
        )
    }

    /// Tests the arms one by one. The checker guarantees that every variant
    /// is handled, so the last arm needs no test.
    fn match_arms(&mut self, subject: Reg, tag: Reg, arms: &'p [MatchArm], span: Span) {
        let [first, rest @ ..] = arms else {
            return;
        };
        if rest.is_empty() || first.pattern == Pattern::Wildcard {
            self.arm_body(subject, first);
            return;
        }

        let code = self.variant_code(subject, &first.pattern);
        let wanted = self.value(Op::Const(Constant::Int(code)), INT, span);
        let cond = self.value(
            Op::Binary {
                op: BinOp::Eq,
                a: tag,
                b: wanted,
            },
            BOOL,
            span,
        );
        self.branch(
            cond,
            |b| b.arm_body(subject, first),
            |b| b.match_arms(subject, tag, rest, span),
        );
    }

    /// What `Op::VariantOf` gives for a value this pattern accepts.
    fn variant_code(&self, subject: Reg, pattern: &Pattern) -> i64 {
        match pattern {
            Pattern::None => 0,
            Pattern::Some(_) => 1,
            Pattern::Variant { variant, .. } => {
                let ty = self.reg_type(subject);
                let Some((name, _)) = nominal(&ty) else {
                    panic!("a variant of a non-enum, the checker should have rejected it");
                };
                let id = self.ctx.structs[name];
                self.ctx.layouts[id]
                    .fields
                    .iter()
                    .position(|v| *v == variant.name)
                    .expect("checked by resolve") as i64
            }
            Pattern::Wildcard => unreachable!("a wildcard is not tested"),
        }
    }

    /// The types of the values a pattern takes out of `subject`.
    fn binding_types(&self, subject: Reg, pattern: &Pattern) -> Vec<Ty> {
        let ty = self.reg_type(subject);
        match (pattern, &ty, nominal(&ty)) {
            (Pattern::Some(_), Ty::Optional(inner), _) => vec![(**inner).clone()],
            (Pattern::Variant { .. }, _, Some((name, args))) => {
                let code = self.variant_code(subject, pattern) as usize;
                let params = &self.ctx.type_params[name];
                self.ctx.variant_types[name][code]
                    .iter()
                    .map(|field| substitute(field, params, args))
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    fn arm_body(&mut self, subject: Reg, arm: &'p MatchArm) {
        self.scopes.push(HashMap::new());
        let base = self.vars.len();
        let types = self.binding_types(subject, &arm.pattern);
        for (index, (name, ty)) in arm.pattern.bindings().iter().zip(types).enumerate() {
            if name.name != "_" {
                let field = Op::VariantField {
                    base: subject,
                    index,
                };
                let value = self.value(field, ty, arm.span);
                self.declare(&name.name, Some(value));
            }
        }
        self.block(&arm.body);
        self.scopes.pop();
        self.vars.truncate(base);
    }

    fn if_stmt(&mut self, cond: &'p Expr, then: &'p ast::Block, otherwise: Option<&'p ast::Block>) {
        let cond = self.expr(cond);
        self.branch(
            cond,
            |b| b.block(then),
            |b| {
                if let Some(otherwise) = otherwise {
                    b.block(otherwise);
                }
            },
        );
    }

    /// Runs one of two pieces of code and joins the variables afterwards.
    fn branch(
        &mut self,
        cond: Reg,
        then: impl FnOnce(&mut Self),
        otherwise: impl FnOnce(&mut Self),
    ) {
        let then_block = self.new_block(Vec::new());
        let else_block = self.new_block(Vec::new());
        self.terminate(Terminator::Branch {
            cond,
            then: Target {
                block: then_block,
                args: Vec::new(),
            },
            otherwise: Target {
                block: else_block,
                args: Vec::new(),
            },
        });

        let before = self.vars.clone();

        self.switch_to(then_block);
        then(self);
        let then_end = (!self.terminated).then_some(self.current);
        let then_vars = std::mem::replace(&mut self.vars, before);

        self.switch_to(else_block);
        otherwise(self);
        let else_end = (!self.terminated).then_some(self.current);
        let else_vars = std::mem::take(&mut self.vars);

        match (then_end, else_end) {
            (None, None) => {
                // Both ended in `return`, `break` or `continue`.
                self.vars = else_vars;
                self.terminated = true;
            }
            (Some(end), None) => self.continue_after(end, then_vars),
            (None, Some(end)) => self.continue_after(end, else_vars),
            (Some(then_end), Some(else_end)) => {
                self.merge(then_end, then_vars, else_end, else_vars);
            }
        }
    }

    /// Only one branch reaches the code after an `if`.
    fn continue_after(&mut self, end: BlockId, vars: Vec<Option<Reg>>) {
        let join = self.new_block(Vec::new());
        self.current = end;
        self.jump(join, Vec::new());
        self.vars = vars;
        self.switch_to(join);
    }

    /// Both branches reach the code after an `if`.
    fn merge(
        &mut self,
        then_end: BlockId,
        then_vars: Vec<Option<Reg>>,
        else_end: BlockId,
        else_vars: Vec<Option<Reg>>,
    ) {
        let mut merged = Vec::with_capacity(then_vars.len());
        let (mut params, mut then_args, mut else_args) = (Vec::new(), Vec::new(), Vec::new());

        for (then_reg, else_reg) in then_vars.into_iter().zip(else_vars) {
            merged.push(match (then_reg, else_reg) {
                (Some(a), Some(b)) if a == b => Some(a),
                (Some(a), Some(b)) => {
                    let ty = self.reg_type(a);
                    let param = self.new_reg(ty);
                    params.push(param);
                    then_args.push(a);
                    else_args.push(b);
                    Some(param)
                }
                // Assigned on one path only, so it cannot be read yet.
                _ => None,
            });
        }

        let join = self.new_block(params);
        self.current = then_end;
        self.jump(join, then_args);
        self.current = else_end;
        self.jump(join, else_args);
        self.vars = merged;
        self.switch_to(join);
    }

    fn while_stmt(&mut self, cond: &'p Expr, body: &'p ast::Block) {
        let assigned = self.assigned_in(body);

        let header_params = self.params_for(&assigned);
        let header = self.new_block(header_params.clone());
        let entry_args = self.registers_of(&assigned);
        self.jump(header, entry_args);

        self.switch_to(header);
        self.bind(&assigned, &header_params);
        let cond = self.expr(cond);

        let exit_params = self.params_for(&assigned);
        let exit = self.new_block(exit_params.clone());
        let body_block = self.new_block(Vec::new());
        self.terminate(Terminator::Branch {
            cond,
            then: Target {
                block: body_block,
                args: Vec::new(),
            },
            otherwise: Target {
                block: exit,
                args: header_params,
            },
        });

        self.loops.push(Loop {
            next: header,
            exit,
            assigned: assigned.clone(),
        });
        self.switch_to(body_block);
        self.block(body);
        if !self.terminated {
            let args = self.registers_of(&assigned);
            self.jump(header, args);
        }
        self.loops.pop();

        self.switch_to(exit);
        self.bind(&assigned, &exit_params);
    }

    /// A `for` over `start..end`, or over `elements` when given: then the
    /// loop variable is the element, not the counter.
    fn for_loop(
        &mut self,
        var: &'p Ident,
        body: &'p ast::Block,
        range: Range,
        elements: Option<Reg>,
        span: Span,
    ) {
        let Range {
            start,
            end,
            step,
            inclusive,
        } = range;
        let assigned = self.assigned_in(body);
        let counter = self.new_reg(INT);

        let header_params = self.params_for(&assigned);
        let mut params = vec![counter];
        params.extend(&header_params);
        let header = self.new_block(params);

        let mut entry_args = vec![start];
        entry_args.extend(self.registers_of(&assigned));
        self.jump(header, entry_args);

        self.switch_to(header);
        self.bind(&assigned, &header_params);
        let more = self.value(
            Op::RangeContinues {
                value: counter,
                end,
                step,
                inclusive,
            },
            BOOL,
            span,
        );

        let exit_params = self.params_for(&assigned);
        let exit = self.new_block(exit_params.clone());
        let body_block = self.new_block(Vec::new());
        self.terminate(Terminator::Branch {
            cond: more,
            then: Target {
                block: body_block,
                args: Vec::new(),
            },
            otherwise: Target {
                block: exit,
                args: header_params,
            },
        });

        // `continue` comes here, to step the counter.
        let latch_params = self.params_for(&assigned);
        let latch = self.new_block(latch_params.clone());
        self.loops.push(Loop {
            next: latch,
            exit,
            assigned: assigned.clone(),
        });

        self.switch_to(body_block);
        self.scopes.push(HashMap::new());
        let base = self.vars.len();
        let item = match elements {
            Some(array) => {
                let elem = element_type(self.reg_type(array).clone());
                self.value(
                    Op::Index {
                        base: array,
                        index: counter,
                    },
                    elem,
                    span,
                )
            }
            None => counter,
        };
        self.declare(&var.name, Some(item));
        self.block(body);
        if !self.terminated {
            let args = self.registers_of(&assigned);
            self.jump(latch, args);
        }
        self.scopes.pop();
        self.vars.truncate(base);
        self.loops.pop();

        // A range stops at the last value that fits an `int`.
        self.switch_to(latch);
        let overflows = self.value(
            Op::AddOverflows {
                a: counter,
                b: step,
            },
            BOOL,
            span,
        );
        self.bind(&assigned, &latch_params);
        let step_params = self.params_for(&assigned);
        let step_block = self.new_block(step_params.clone());
        self.terminate(Terminator::Branch {
            cond: overflows,
            then: Target {
                block: exit,
                args: latch_params.clone(),
            },
            otherwise: Target {
                block: step_block,
                args: latch_params,
            },
        });

        self.switch_to(step_block);
        let next = self.value(
            Op::Binary {
                op: BinOp::Add,
                a: counter,
                b: step,
            },
            INT,
            span,
        );
        let mut args = vec![next];
        args.extend(step_params);
        self.jump(header, args);

        self.switch_to(exit);
        self.bind(&assigned, &exit_params);
    }

    // ----- expressions -----

    fn ty(&self, e: &Expr) -> Ty {
        self.ctx
            .types
            .of(self.item, e.span)
            .expect("the checker records every expression")
            .clone()
    }

    fn expr(&mut self, e: &'p Expr) -> Reg {
        let ty = self.ty(e);
        match &e.kind {
            ExprKind::Int(v) => self.value(Op::Const(Constant::Int(*v)), ty, e.span),
            ExprKind::Real(v) => self.value(Op::Const(Constant::Real(*v)), ty, e.span),
            ExprKind::Complex { re, im } => self.value(
                Op::Const(Constant::Complex { re: *re, im: *im }),
                ty,
                e.span,
            ),
            ExprKind::Bool(v) => self.value(Op::Const(Constant::Bool(*v)), ty, e.span),
            ExprKind::Str(text) => self.value(Op::Const(Constant::Str(text.clone())), ty, e.span),
            ExprKind::Var(name) => self.variable(name, ty, e.span),
            ExprKind::Unary { op, operand } => self.unary(*op, operand, ty, e.span),
            ExprKind::Binary {
                op: op @ (BinOp::And | BinOp::Or),
                lhs,
                rhs,
            } => self.short_circuit(*op, lhs, rhs),
            ExprKind::Binary { op, lhs, rhs } => {
                let a = self.expr(lhs);
                let b = self.expr(rhs);
                self.value(Op::Binary { op: *op, a, b }, ty, e.span)
            }
            ExprKind::Call { name, args } => self
                .call(name, args, Some(ty), e.span)
                .expect("the checker ensures the function returns a value"),
            ExprKind::Swizzle { base, mask } => {
                let base = self.expr(base);
                let mask = mask.name.clone();
                self.value(Op::Swizzle { base, mask }, ty, e.span)
            }
            ExprKind::Field { base, name } => {
                let base_ty = self.ty(base);
                let base = self.expr(base);
                let (index, _) = self.field(&base_ty, &name.name);
                self.value(Op::FieldGet { base, index }, ty, e.span)
            }
            ExprKind::StructLit { name, fields } => self.struct_literal(name, fields, ty, e.span),
            ExprKind::Variant {
                name,
                variant,
                args,
            } => {
                let values = args.iter().map(|arg| self.expr(arg)).collect();
                let id = self.ctx.structs[name.name.as_str()];
                let variant = self.ctx.layouts[id]
                    .fields
                    .iter()
                    .position(|v| *v == variant.name)
                    .expect("checked by resolve");
                self.value(
                    Op::MakeEnum {
                        id,
                        variant,
                        values,
                    },
                    ty,
                    e.span,
                )
            }
            ExprKind::Some(value) => {
                let value = self.expr(value);
                self.value(Op::MakeSome(value), ty, e.span)
            }
            ExprKind::None => self.value(Op::MakeNone, ty, e.span),
            ExprKind::Lambda(l) => self.lambda(l, ty, e.span),
            ExprKind::Tuple(items) => {
                let items = items.iter().map(|item| self.expr(item)).collect();
                self.value(Op::MakeTuple(items), ty, e.span)
            }
            ExprKind::Array(items) => {
                let items = items.iter().map(|item| self.expr(item)).collect();
                self.value(Op::MakeArray(items), ty, e.span)
            }
            ExprKind::ArrayFill { value, len } => {
                let value = self.expr(value);
                let length = self.expr(len);
                self.value(Op::ArrayFill { value, len: length }, ty, len.span)
            }
            ExprKind::Index { base, index } => {
                let base = self.expr(base);
                let position = self.expr(index);
                self.value(
                    Op::Index {
                        base,
                        index: position,
                    },
                    ty,
                    index.span,
                )
            }
            // A matrix takes two steps of a path where an array takes one.
            ExprKind::Cell { base, row, col } => {
                let base = self.expr(base);
                let path = vec![
                    Step::Index(self.expr(row), row.span),
                    Step::Index(self.expr(col), col.span),
                ];
                self.value(Op::PathGet { base, path }, ty, e.span)
            }
            ExprKind::Slice {
                base,
                start,
                end,
                inclusive,
            } => {
                let base = self.expr(base);
                let start = start.as_ref().map(|start| self.expr(start));
                let end = end.as_ref().map(|end| self.expr(end));
                self.value(
                    Op::Slice {
                        base,
                        start,
                        end,
                        inclusive: *inclusive,
                    },
                    ty,
                    e.span,
                )
            }
        }
    }

    fn variable(&mut self, name: &Ident, ty: Ty, span: Span) -> Reg {
        if let Some(var) = self.lookup(&name.name) {
            return self.register_of(var);
        }
        if let Some(&id) = self.ctx.globals.get(name.name.as_str()) {
            return self.value(Op::GetGlobal(id), ty, span);
        }
        if let Some(&variant) = self.ctx.named.get(name.name.as_str()) {
            return self.function_value(variant, Vec::new(), ty, span);
        }

        let builtin = kmpl_builtins::lookup_const(&name.name).expect("checked by resolve");
        let Value::Real(value) = builtin.value else {
            panic!("built-in constants are real numbers");
        };
        self.value(Op::Const(Constant::Real(value)), ty, span)
    }

    fn unary(&mut self, op: UnOp, operand: &'p Expr, ty: Ty, span: Span) -> Reg {
        let a = self.expr(operand);
        self.value(Op::Unary { op, a }, ty, span)
    }

    /// `false && x` and `true || x` do not run `x`.
    fn short_circuit(&mut self, op: BinOp, lhs: &'p Expr, rhs: &'p Expr) -> Reg {
        let left = self.expr(lhs);
        let result = self.new_reg(BOOL);
        let join = self.new_block(vec![result]);
        let right_block = self.new_block(Vec::new());

        let evaluate_right = Target {
            block: right_block,
            args: Vec::new(),
        };
        let skip = Target {
            block: join,
            args: vec![left],
        };
        let (then, otherwise) = match op {
            BinOp::And => (evaluate_right, skip),
            _ => (skip, evaluate_right),
        };
        self.terminate(Terminator::Branch {
            cond: left,
            then,
            otherwise,
        });

        self.switch_to(right_block);
        let right = self.expr(rhs);
        self.jump(join, vec![right]);

        self.switch_to(join);
        result
    }

    /// With no `ty` the result is dropped: the call is a statement.
    fn function_value(&mut self, variant: usize, values: Vec<Reg>, ty: Ty, span: Span) -> Reg {
        let id = self.ctx.function_layout;
        self.value(
            Op::MakeEnum {
                id,
                variant,
                values,
            },
            ty,
            span,
        )
    }

    /// Remembers what the function reads from around it, in the order of
    /// the names, and leaves its body for later.
    fn lambda(&mut self, l: &'p ast::Lambda, ty: Ty, span: Span) -> Reg {
        let mut names = Vec::new();
        visit_block(&l.body, &mut |e: &'p Expr| match &e.kind {
            ExprKind::Var(name) | ExprKind::Call { name, .. } => names.push(name.name.as_str()),
            _ => {}
        });
        names.sort_unstable();
        names.dedup();

        let mut values = Vec::new();
        let mut captured = Vec::new();
        for name in names {
            if let Some(reg) = self.lookup(name).and_then(|var| self.vars[var]) {
                values.push(reg);
                captured.push((name, self.reg_type(reg)));
            }
        }

        let variant = self.ctx.lambdas[&(self.item, span)];
        let func = self.ctx.candidates[variant].func;
        self.ctx.pending.push(Pending {
            func,
            lambda: l,
            item: self.item,
            captured,
            signature: self.ctx.candidates[variant].signature.clone(),
        });
        self.function_value(variant, values, ty, span)
    }

    /// The function value held by `name`, if that is what it holds.
    fn held_function(&mut self, name: &Ident, span: Span) -> Option<Reg> {
        let is_function = |ty: &Ty| matches!(ty, Ty::Function { .. });
        if let Some(var) = self.lookup(&name.name) {
            let reg = self.register_of(var);
            return is_function(&self.reg_type(reg)).then_some(reg);
        }
        let ty = self.ctx.const_types.get(name.name.as_str())?.clone();
        if !is_function(&ty) {
            return None;
        }
        let id = self.ctx.globals[name.name.as_str()];
        Some(self.value(Op::GetGlobal(id), ty, span))
    }

    /// Chooses among the functions with this signature by the variant the
    /// value holds. The last one needs no test.
    fn call_value(
        &mut self,
        callee: Reg,
        args: Vec<Reg>,
        ty: Option<Ty>,
        span: Span,
    ) -> Option<Reg> {
        let signature = self.reg_type(callee);
        let choices: Vec<usize> = (0..self.ctx.candidates.len())
            .filter(|&c| self.ctx.candidates[c].signature == signature)
            .collect();

        let result = ty.as_ref().map(|ty| self.new_reg(ty.clone()));
        let join = self.new_block(result.into_iter().collect());
        let tag = self.value(Op::VariantOf(callee), INT, span);

        for (position, &choice) in choices.iter().enumerate() {
            if position + 1 < choices.len() {
                let wanted = self.value(Op::Const(Constant::Int(choice as i64)), INT, span);
                let cond = self.value(
                    Op::Binary {
                        op: BinOp::Eq,
                        a: tag,
                        b: wanted,
                    },
                    BOOL,
                    span,
                );
                let then_block = self.new_block(Vec::new());
                let else_block = self.new_block(Vec::new());
                self.terminate(Terminator::Branch {
                    cond,
                    then: Target {
                        block: then_block,
                        args: Vec::new(),
                    },
                    otherwise: Target {
                        block: else_block,
                        args: Vec::new(),
                    },
                });
                self.switch_to(then_block);
                self.call_choice(choice, callee, &args, ty.as_ref(), join, span);
                self.switch_to(else_block);
            } else {
                self.call_choice(choice, callee, &args, ty.as_ref(), join, span);
            }
        }
        if choices.is_empty() {
            // No function of this type exists, so there cannot be a value.
            self.terminate(Terminator::Unreachable);
        }

        self.switch_to(join);
        result
    }

    fn call_choice(
        &mut self,
        choice: usize,
        callee: Reg,
        args: &[Reg],
        ty: Option<&Ty>,
        join: BlockId,
        span: Span,
    ) {
        let func = self.ctx.candidates[choice].func;
        let mut all = Vec::with_capacity(args.len() + 1);
        if self.ctx.candidates[choice].takes_itself {
            all.push(callee);
        }
        all.extend_from_slice(args);

        let op = Op::Call { func, args: all };
        match ty {
            Some(ty) => {
                let value = self.value(op, ty.clone(), span);
                self.jump(join, vec![value]);
            }
            None => {
                self.effect(op, span);
                self.jump(join, Vec::new());
            }
        }
    }

    fn call(&mut self, name: &Ident, args: &'p [Expr], ty: Option<Ty>, span: Span) -> Option<Reg> {
        let held = self.held_function(name, span);
        let args: Vec<Reg> = args.iter().map(|arg| self.expr(arg)).collect();
        if let Some(callee) = held {
            return self.call_value(callee, args, ty, span);
        }

        let op = match self.ctx.functions.get(name.name.as_str()) {
            Some(&func) => Op::Call { func, args },
            None => Op::Builtin {
                id: self.ctx.builtin_id(&name.name),
                args,
            },
        };

        match ty {
            Some(ty) => Some(self.value(op, ty, span)),
            None => {
                self.effect(op, span);
                None
            }
        }
    }

    /// Fields run in the order they are written, but are stored in the
    /// order of the definition.
    fn struct_literal(
        &mut self,
        name: &Ident,
        fields: &'p [(Ident, Expr)],
        ty: Ty,
        span: Span,
    ) -> Reg {
        let id = self.ctx.structs[name.name.as_str()];
        let mut values = vec![None; self.ctx.layouts[id].fields.len()];

        for (field, value) in fields {
            let reg = self.expr(value);
            let (index, _) = self.field(&ty, &field.name);
            values[index] = Some(reg);
        }

        let values = values
            .into_iter()
            .map(|v| v.expect("the checker ensures every field is given"))
            .collect();
        self.value(Op::MakeStruct { id, values }, ty, span)
    }
}

/// Names assigned anywhere in `block`, including nested blocks.
fn collect_assigned<'p>(block: &'p ast::Block, names: &mut HashSet<&'p str>) {
    for stmt in block {
        match &stmt.kind {
            StmtKind::Assign { name, .. } | StmtKind::PathAssign { name, .. } => {
                names.insert(&name.name);
            }
            StmtKind::If {
                then, otherwise, ..
            } => {
                collect_assigned(then, names);
                if let Some(otherwise) = otherwise {
                    collect_assigned(otherwise, names);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::ForEach { body, .. } => collect_assigned(body, names),
            StmtKind::Match { arms, .. } => {
                for arm in arms {
                    collect_assigned(&arm.body, names);
                }
            }
            StmtKind::Let { .. }
            | StmtKind::LetTuple { .. }
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Return(_)
            | StmtKind::Expr(_) => {}
        }
    }
}

/// What indexing a value of type `ty` gives.
fn element_type(ty: Ty) -> Ty {
    match ty {
        Ty::Array(element) => *element,
        Ty::Prim(Type::Bytes) => INT,
        Ty::Prim(Type::Str) => Ty::Prim(Type::Str),
        other => panic!("indexing {other:?}, the checker should have rejected it"),
    }
}
