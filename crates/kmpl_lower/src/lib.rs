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
        TypeExpr::Tuple(items) => Ty::Tuple(items.iter().map(to_ty).collect()),
    }
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
            }
            Item::Struct(s) => {
                ctx.structs.insert(&s.name.name, ctx.layouts.len());
                ctx.layouts.push(StructLayout {
                    name: s.name.name.clone(),
                    fields: s.fields.iter().map(|f| f.name.name.clone()).collect(),
                });
                ctx.field_types
                    .push(s.fields.iter().map(|f| to_ty(&f.ty)).collect());
            }
            Item::Enum(e) => {
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

    let mut functions = Vec::new();
    for (index, item) in program.items.iter().enumerate() {
        if let Item::Function(f) = item {
            functions.push(lower_function(&mut ctx, index, f));
        }
    }
    let init = FuncId(functions.len() as u32);
    functions.push(lower_constants(&mut ctx, program));

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
            StmtKind::Assign { name, op, value } => {
                let value = self.expr(value);
                let var = self.lookup(&name.name).expect("checked by resolve");
                let result = match op {
                    Some(op) => {
                        let old = self.register_of(var);
                        let ty = self.reg_type(old);
                        self.value(
                            Op::Binary {
                                op: *op,
                                a: old,
                                b: value,
                            },
                            ty,
                            stmt.span,
                        )
                    }
                    None => value,
                };
                self.vars[var] = Some(result);
            }
            StmtKind::PathAssign {
                name,
                path,
                op,
                value,
            } => self.path_assign(name, path, *op, value, stmt.span),
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
                    if stmt.is_panic() {
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
            value = self.value(
                Op::Binary {
                    op,
                    a: old,
                    b: value,
                },
                ty,
                span,
            );
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

    /// Position and type of a field of the struct type `ty`.
    fn field(&self, ty: &Ty, name: &str) -> (usize, Ty) {
        let Ty::Struct(struct_name) = ty else {
            panic!("a field of a non-struct, the checker should have rejected it");
        };
        let id = self.ctx.structs[struct_name.as_str()];
        let index = self.ctx.layouts[id]
            .fields
            .iter()
            .position(|f| f == name)
            .expect("checked by inference");
        (index, self.ctx.field_types[id][index].clone())
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
                let Ty::Struct(name) = self.reg_type(subject) else {
                    panic!("a variant of a non-enum, the checker should have rejected it");
                };
                let id = self.ctx.structs[name.as_str()];
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
        match (pattern, self.reg_type(subject)) {
            (Pattern::Some(_), Ty::Optional(inner)) => vec![*inner],
            (Pattern::Variant { .. }, Ty::Struct(name)) => {
                let code = self.variant_code(subject, pattern) as usize;
                self.ctx.variant_types[name.as_str()][code].clone()
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
    fn call(&mut self, name: &Ident, args: &'p [Expr], ty: Option<Ty>, span: Span) -> Option<Reg> {
        let args: Vec<Reg> = args.iter().map(|arg| self.expr(arg)).collect();

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
