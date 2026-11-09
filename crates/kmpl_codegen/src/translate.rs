//! Translates IR functions to LLVM functions. Both are SSA, and a block's
//! parameters become phi nodes.

use std::cell::RefCell;
use std::collections::HashMap;

use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::{Builder, BuilderError};
use inkwell::context::Context;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::{Linkage, Module};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PhiValue, PointerValue};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};
use kmpl_ast::{BinOp, Span, Ty, Type, UnOp};
use kmpl_builtins::{BUILTINS, abi, ops};
use kmpl_ir::{Constant, Function, Inst, Op, Reg, Target, Terminator};

/// The symbol `kmpl_rt` calls to start the program.
const ENTRY: &str = "kmpl_program";

type Result<T> = std::result::Result<T, BuilderError>;

pub struct Translator<'ctx, 'm> {
    context: &'ctx Context,
    llvm: Module<'ctx>,
    builder: Builder<'ctx>,
    ir: &'m kmpl_ir::Module,
    locate: &'m dyn Fn(Span) -> String,
    functions: Vec<FunctionValue<'ctx>>,
    globals: Vec<Option<PointerValue<'ctx>>>,
    /// How many calls are active, for the same limit the VM has.
    depth: PointerValue<'ctx>,
    runtime_error: FunctionValue<'ctx>,
    runtime_builtin: FunctionValue<'ctx>,
    /// For each entry of `Module::builtins`, its position in `BUILTINS`.
    builtins: Vec<u64>,
    strings: RefCell<HashMap<String, PointerValue<'ctx>>>,
}

/// What is known about the LLVM function being generated.
struct Current<'ctx, 'm> {
    ir: &'m Function,
    llvm: FunctionValue<'ctx>,
    /// The block that only holds stack slots.
    start: BasicBlock<'ctx>,
    blocks: Vec<BasicBlock<'ctx>>,
    phis: Vec<Vec<PhiValue<'ctx>>>,
    values: Vec<Option<BasicValueEnum<'ctx>>>,
    /// Every jump, as its target and the block it leaves from.
    edges: Vec<(&'m Target, BasicBlock<'ctx>)>,
}

impl<'ctx> Current<'ctx, '_> {
    fn value(&self, reg: Reg) -> BasicValueEnum<'ctx> {
        self.values[reg.0 as usize].expect("a register is defined before it is used")
    }

    fn int(&self, reg: Reg) -> IntValue<'ctx> {
        self.value(reg).into_int_value()
    }

    fn ty(&self, reg: Reg) -> &'_ Ty {
        &self.ir.types[reg.0 as usize]
    }
}

impl<'ctx, 'm> Translator<'ctx, 'm> {
    pub fn new(
        context: &'ctx Context,
        ir: &'m kmpl_ir::Module,
        locate: &'m dyn Fn(Span) -> String,
    ) -> Self {
        let llvm = context.create_module("kmpl");
        let void = context.void_type();
        let i64_type = context.i64_type();
        let ptr: BasicMetadataTypeEnum = context.ptr_type(AddressSpace::default()).into();
        let int: BasicMetadataTypeEnum = i64_type.into();

        let runtime_error = llvm.add_function(
            "kmpl_rt_error",
            void.fn_type(&[ptr, int, ptr, int], false),
            None,
        );
        for attribute in ["noreturn", "cold"] {
            let kind = Attribute::get_named_enum_kind_id(attribute);
            runtime_error.add_attribute(
                AttributeLoc::Function,
                context.create_enum_attribute(kind, 0),
            );
        }
        let runtime_builtin = llvm.add_function(
            "kmpl_rt_builtin",
            void.fn_type(&[int, ptr, int, ptr, ptr, int], false),
            None,
        );

        let depth = llvm.add_global(i64_type, None, "kmpl.depth");
        depth.set_linkage(Linkage::Private);
        depth.set_initializer(&i64_type.const_zero());

        let mut translator = Translator {
            context,
            builder: context.create_builder(),
            ir,
            locate,
            functions: Vec::new(),
            globals: Vec::new(),
            depth: depth.as_pointer_value(),
            runtime_error,
            runtime_builtin,
            builtins: ir
                .builtins
                .iter()
                .map(|name| {
                    BUILTINS
                        .iter()
                        .position(|b| b.name == name)
                        .expect("lowering found this built-in") as u64
                })
                .collect(),
            strings: RefCell::new(HashMap::new()),
            llvm,
        };
        translator.declare_globals();
        translator.declare_functions();
        translator
    }

    /// Generates everything and hands over the finished module.
    pub fn translate(self) -> Result<Module<'ctx>> {
        for index in 0..self.ir.functions.len() {
            self.ir_function(index)?;
        }
        self.entry()?;
        Ok(self.llvm)
    }

    fn llvm_type(&self, ty: &Ty) -> BasicTypeEnum<'ctx> {
        match ty {
            Ty::Prim(Type::Int) => self.context.i64_type().into(),
            Ty::Prim(Type::Real) => self.context.f64_type().into(),
            Ty::Prim(Type::Bool) => self.context.bool_type().into(),
            other => unreachable!("`supported` rejected {other:?}"),
        }
    }

    /// The IR has no list of global types, so they come from their uses.
    fn declare_globals(&mut self) {
        let mut types: Vec<Option<&Ty>> = vec![None; self.ir.globals];
        for function in &self.ir.functions {
            for inst in function.blocks.iter().flat_map(|b| &b.insts) {
                match (&inst.op, inst.dest) {
                    (Op::SetGlobal(id, reg), _) => {
                        types[*id] = Some(&function.types[reg.0 as usize])
                    }
                    (Op::GetGlobal(id), Some(dest)) => {
                        types[*id] = Some(&function.types[dest.0 as usize]);
                    }
                    _ => {}
                }
            }
        }

        self.globals = types
            .into_iter()
            .enumerate()
            .map(|(id, ty)| {
                let ty = self.llvm_type(ty?);
                let global = self.llvm.add_global(ty, None, &format!("kmpl.global.{id}"));
                global.set_linkage(Linkage::Private);
                global.set_initializer(&ty.const_zero());
                Some(global.as_pointer_value())
            })
            .collect();
    }

    fn declare_functions(&mut self) {
        for (index, function) in self.ir.functions.iter().enumerate() {
            let params: Vec<BasicMetadataTypeEnum> = function
                .params
                .iter()
                .map(|reg| self.llvm_type(&function.types[reg.0 as usize]).into())
                .collect();
            let signature = match &function.ret {
                Some(ty) => self.llvm_type(ty).fn_type(&params, false),
                None => self.context.void_type().fn_type(&params, false),
            };
            let name = format!("kmpl.{index}.{}", function.name);
            self.functions.push(
                self.llvm
                    .add_function(&name, signature, Some(Linkage::Private)),
            );
        }
    }

    /// Runs the constants, then `main`.
    fn entry(&self) -> Result<()> {
        let signature = self.context.void_type().fn_type(&[], false);
        let entry = self.llvm.add_function(ENTRY, signature, None);
        self.builder
            .position_at_end(self.context.append_basic_block(entry, "start"));
        for function in [self.ir.init, self.ir.main] {
            self.builder
                .build_call(self.functions[function.0 as usize], &[], "")?;
        }
        self.builder.build_return(None)?;
        Ok(())
    }

    fn ir_function(&self, index: usize) -> Result<()> {
        let ir = &self.ir.functions[index];
        let llvm = self.functions[index];
        let start = self.context.append_basic_block(llvm, "start");
        let blocks: Vec<BasicBlock> = ir
            .blocks
            .iter()
            .map(|_| self.context.append_basic_block(llvm, ""))
            .collect();
        let mut current = Current {
            ir,
            llvm,
            start,
            blocks,
            phis: vec![Vec::new(); ir.blocks.len()],
            values: vec![None; ir.types.len()],
            edges: Vec::new(),
        };

        // Only blocks that can run, each after the blocks that dominate it,
        // so a register is always defined before it is read.
        let order = reverse_postorder(ir);
        for &block in &order {
            self.builder.position_at_end(current.blocks[block]);
            for param in &ir.blocks[block].params {
                let phi = self
                    .builder
                    .build_phi(self.llvm_type(&ir.types[param.0 as usize]), "")?;
                current.values[param.0 as usize] = Some(phi.as_basic_value());
                current.phis[block].push(phi);
            }
        }

        self.builder.position_at_end(start);
        self.builder.build_unconditional_branch(current.blocks[0])?;
        for (position, param) in ir.params.iter().enumerate() {
            let argument = llvm
                .get_nth_param(position as u32)
                .expect("declared with these parameters");
            match ir.blocks[0].params.iter().position(|p| p == param) {
                Some(phi) => current.phis[0][phi].add_incoming(&[(&argument, start)]),
                None => current.values[param.0 as usize] = Some(argument),
            }
        }

        for &block in &order {
            self.builder.position_at_end(current.blocks[block]);
            for inst in &ir.blocks[block].insts {
                self.inst(inst, &mut current)?;
            }
            self.terminator(&ir.blocks[block].term, &mut current)?;
        }

        // Last, because a jump back up a loop passes values defined later.
        let edges = std::mem::take(&mut current.edges);
        for (target, from) in edges {
            let phis = &current.phis[target.block.0 as usize];
            for (phi, arg) in phis.iter().zip(&target.args) {
                phi.add_incoming(&[(&current.value(*arg), from)]);
            }
        }
        Ok(())
    }

    fn terminator(&self, term: &'m Terminator, current: &mut Current<'ctx, 'm>) -> Result<()> {
        let builder = &self.builder;
        match term {
            Terminator::Jump(target) => {
                let from = builder.get_insert_block().expect("positioned");
                builder.build_unconditional_branch(current.blocks[target.block.0 as usize])?;
                current.edges.push((target, from));
            }
            Terminator::Branch {
                cond,
                then,
                otherwise,
            } => {
                let condition = current.int(*cond);
                let here = builder.get_insert_block().expect("positioned");
                let (then_from, else_from) = if then.block == otherwise.block {
                    // A phi cannot tell two edges from one block apart, so
                    // each way gets a block of its own.
                    let join = current.blocks[then.block.0 as usize];
                    let then_edge = self.context.append_basic_block(current.llvm, "");
                    let else_edge = self.context.append_basic_block(current.llvm, "");
                    builder.build_conditional_branch(condition, then_edge, else_edge)?;
                    for edge in [then_edge, else_edge] {
                        builder.position_at_end(edge);
                        builder.build_unconditional_branch(join)?;
                    }
                    (then_edge, else_edge)
                } else {
                    builder.build_conditional_branch(
                        condition,
                        current.blocks[then.block.0 as usize],
                        current.blocks[otherwise.block.0 as usize],
                    )?;
                    (here, here)
                };
                current.edges.push((then, then_from));
                current.edges.push((otherwise, else_from));
            }
            Terminator::Return(Some(reg)) => {
                builder.build_return(Some(&current.value(*reg)))?;
            }
            Terminator::Return(None) => {
                builder.build_return(None)?;
            }
            Terminator::Unreachable => {
                builder.build_unreachable()?;
            }
        }
        Ok(())
    }

    fn inst(&self, inst: &Inst, current: &mut Current<'ctx, 'm>) -> Result<()> {
        let result = self.operation(inst, current)?;
        if let (Some(dest), Some(value)) = (inst.dest, result) {
            current.values[dest.0 as usize] = Some(value);
        }
        Ok(())
    }

    fn operation(
        &self,
        inst: &Inst,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        let span = inst.span;
        let value: BasicValueEnum = match &inst.op {
            Op::Const(constant) => self.constant(constant),
            Op::Unary { op, a } => self.unary(*op, *a, span, current)?,
            Op::Binary { op, a, b } => self.binary(*op, *a, *b, span, current)?,
            Op::Call { func, args } => return self.call(func.0 as usize, args, span, current),
            Op::Builtin { id, args } => {
                return self.builtin(*id, args, inst.dest, span, current);
            }
            Op::GetGlobal(id) => {
                let dest = inst.dest.expect("reading a global has a result");
                let ty = self.llvm_type(current.ty(dest));
                let global = self.globals[*id].expect("this use gave the global a type");
                self.builder.build_load(ty, global, "")?
            }
            Op::SetGlobal(id, reg) => {
                let global = self.globals[*id].expect("this use gave the global a type");
                self.builder.build_store(global, current.value(*reg))?;
                return Ok(None);
            }
            Op::CheckStep(step) => {
                let zero = self.context.i64_type().const_zero();
                let is_zero = self.builder.build_int_compare(
                    IntPredicate::EQ,
                    current.int(*step),
                    zero,
                    "",
                )?;
                self.fail_if(is_zero, ops::ZERO_STEP, span, current)?;
                return Ok(None);
            }
            Op::RangeContinues {
                value,
                end,
                step,
                inclusive,
            } => {
                let (v, end, step) = (current.int(*value), current.int(*end), current.int(*step));
                let (up, down) = match inclusive {
                    false => (IntPredicate::SLT, IntPredicate::SGT),
                    true => (IntPredicate::SLE, IntPredicate::SGE),
                };
                let zero = self.context.i64_type().const_zero();
                let upward = self
                    .builder
                    .build_int_compare(IntPredicate::SGT, step, zero, "")?;
                let up = self.builder.build_int_compare(up, v, end, "")?;
                let down = self.builder.build_int_compare(down, v, end, "")?;
                self.builder.build_select(upward, up, down, "")?
            }
            Op::AddOverflows { a, b } => {
                let (_, overflow) = self.with_overflow(
                    "llvm.sadd.with.overflow",
                    current.int(*a),
                    current.int(*b),
                )?;
                overflow.into()
            }
            other => unreachable!("`supported` rejected {other:?}"),
        };
        Ok(Some(value))
    }

    fn constant(&self, constant: &Constant) -> BasicValueEnum<'ctx> {
        match constant {
            Constant::Int(v) => self.context.i64_type().const_int(*v as u64, true).into(),
            Constant::Real(v) => self.context.f64_type().const_float(*v).into(),
            Constant::Bool(v) => self
                .context
                .bool_type()
                .const_int(u64::from(*v), false)
                .into(),
            other => unreachable!("`supported` rejected {other:?}"),
        }
    }

    fn unary(
        &self,
        op: UnOp,
        a: Reg,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let value: BasicValueEnum = match (op, current.ty(a)) {
            (UnOp::Not, _) => self.builder.build_not(current.int(a), "")?.into(),
            (UnOp::Neg, Ty::Prim(Type::Real)) => self
                .builder
                .build_float_neg(current.value(a).into_float_value(), "")?
                .into(),
            (UnOp::Neg, _) => {
                let zero = self.context.i64_type().const_zero();
                self.checked(
                    "llvm.ssub.with.overflow",
                    zero,
                    current.int(a),
                    span,
                    current,
                )?
                .into()
            }
        };
        Ok(value)
    }

    fn binary(
        &self,
        op: BinOp,
        a: Reg,
        b: Reg,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        match current.ty(a) {
            Ty::Prim(Type::Real) => self.real_binary(op, a, b, current),
            _ => self.int_binary(op, current.int(a), current.int(b), span, current),
        }
    }

    fn real_binary(
        &self,
        op: BinOp,
        a: Reg,
        b: Reg,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let builder = &self.builder;
        let (x, y) = (
            current.value(a).into_float_value(),
            current.value(b).into_float_value(),
        );
        let compare = |predicate| -> Result<BasicValueEnum<'ctx>> {
            Ok(builder.build_float_compare(predicate, x, y, "")?.into())
        };
        match op {
            BinOp::Add => Ok(builder.build_float_add(x, y, "")?.into()),
            BinOp::Sub => Ok(builder.build_float_sub(x, y, "")?.into()),
            BinOp::Mul => Ok(builder.build_float_mul(x, y, "")?.into()),
            BinOp::Div => Ok(builder.build_float_div(x, y, "")?.into()),
            BinOp::Eq => compare(FloatPredicate::OEQ),
            // Unordered: `NaN != NaN` is true.
            BinOp::Ne => compare(FloatPredicate::UNE),
            BinOp::Lt => compare(FloatPredicate::OLT),
            BinOp::Le => compare(FloatPredicate::OLE),
            BinOp::Gt => compare(FloatPredicate::OGT),
            BinOp::Ge => compare(FloatPredicate::OGE),
            BinOp::And | BinOp::Or => unreachable!("lowered to branches"),
        }
    }

    /// Also compares `bool`s, which are integers here.
    fn int_binary(
        &self,
        op: BinOp,
        x: IntValue<'ctx>,
        y: IntValue<'ctx>,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<BasicValueEnum<'ctx>> {
        let builder = &self.builder;
        let compare = |predicate| -> Result<BasicValueEnum<'ctx>> {
            Ok(builder.build_int_compare(predicate, x, y, "")?.into())
        };
        match op {
            BinOp::Add => Ok(self
                .checked("llvm.sadd.with.overflow", x, y, span, current)?
                .into()),
            BinOp::Sub => Ok(self
                .checked("llvm.ssub.with.overflow", x, y, span, current)?
                .into()),
            BinOp::Mul => Ok(self
                .checked("llvm.smul.with.overflow", x, y, span, current)?
                .into()),
            BinOp::Div => Ok(self.checked_div(x, y, span, current)?.into()),
            BinOp::Eq => compare(IntPredicate::EQ),
            BinOp::Ne => compare(IntPredicate::NE),
            BinOp::Lt => compare(IntPredicate::SLT),
            BinOp::Le => compare(IntPredicate::SLE),
            BinOp::Gt => compare(IntPredicate::SGT),
            BinOp::Ge => compare(IntPredicate::SGE),
            BinOp::And | BinOp::Or => unreachable!("lowered to branches"),
        }
    }

    fn with_overflow(
        &self,
        name: &str,
        x: IntValue<'ctx>,
        y: IntValue<'ctx>,
    ) -> Result<(IntValue<'ctx>, IntValue<'ctx>)> {
        let declaration = Intrinsic::find(name)
            .expect("LLVM has this intrinsic")
            .get_declaration(&self.llvm, &[self.context.i64_type().into()])
            .expect("the intrinsic takes this type");
        let pair = self
            .builder
            .build_call(declaration, &[x.into(), y.into()], "")?
            .try_as_basic_value()
            .basic()
            .expect("the intrinsic returns a pair")
            .into_struct_value();
        let value = self.builder.build_extract_value(pair, 0, "")?;
        let overflow = self.builder.build_extract_value(pair, 1, "")?;
        Ok((value.into_int_value(), overflow.into_int_value()))
    }

    fn checked(
        &self,
        name: &str,
        x: IntValue<'ctx>,
        y: IntValue<'ctx>,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<IntValue<'ctx>> {
        let (value, overflow) = self.with_overflow(name, x, y)?;
        self.fail_if(overflow, ops::OVERFLOW, span, current)?;
        Ok(value)
    }

    fn checked_div(
        &self,
        x: IntValue<'ctx>,
        y: IntValue<'ctx>,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<IntValue<'ctx>> {
        let builder = &self.builder;
        let i64_type = self.context.i64_type();
        let by_zero = builder.build_int_compare(IntPredicate::EQ, y, i64_type.const_zero(), "")?;
        self.fail_if(by_zero, ops::DIVISION_BY_ZERO, span, current)?;

        // The one division that does not fit: `sdiv` would be undefined.
        let minimum = i64_type.const_int(i64::MIN as u64, true);
        let minus_one = i64_type.const_int(-1i64 as u64, true);
        let is_minimum = builder.build_int_compare(IntPredicate::EQ, x, minimum, "")?;
        let is_minus_one = builder.build_int_compare(IntPredicate::EQ, y, minus_one, "")?;
        let overflow = builder.build_and(is_minimum, is_minus_one, "")?;
        self.fail_if(overflow, ops::OVERFLOW, span, current)?;

        builder.build_int_signed_div(x, y, "")
    }

    /// Stops the program with `message` if `condition` holds. Afterwards the
    /// builder is in a new block, the one where it did not hold.
    fn fail_if(
        &self,
        condition: IntValue<'ctx>,
        message: &str,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<()> {
        let failed = self.context.append_basic_block(current.llvm, "");
        let fine = self.context.append_basic_block(current.llvm, "");
        self.builder
            .build_conditional_branch(condition, failed, fine)?;

        self.builder.position_at_end(failed);
        let (message_ptr, message_len) = self.string(message)?;
        let (location_ptr, location_len) = self.string(&(self.locate)(span))?;
        self.builder.build_call(
            self.runtime_error,
            &[
                message_ptr.into(),
                message_len.into(),
                location_ptr.into(),
                location_len.into(),
            ],
            "",
        )?;
        self.builder.build_unreachable()?;

        self.builder.position_at_end(fine);
        Ok(())
    }

    fn call(
        &self,
        function: usize,
        args: &[Reg],
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        let builder = &self.builder;
        let i64_type = self.context.i64_type();

        let depth = builder
            .build_load(i64_type, self.depth, "")?
            .into_int_value();
        let deeper = builder.build_int_add(depth, i64_type.const_int(1, false), "")?;
        let limit = i64_type.const_int(ops::MAX_CALL_DEPTH as u64, false);
        let too_deep = builder.build_int_compare(IntPredicate::SGE, deeper, limit, "")?;
        self.fail_if(too_deep, &ops::stack_overflow_message(), span, current)?;
        builder.build_store(self.depth, deeper)?;

        let args: Vec<_> = args.iter().map(|r| current.value(*r).into()).collect();
        let result = builder.build_call(self.functions[function], &args, "")?;
        builder.build_store(self.depth, depth)?;
        Ok(result.try_as_basic_value().basic())
    }

    /// Passes the arguments to `kmpl_rt_builtin` as an array of `abi::Slot`.
    fn builtin(
        &self,
        id: usize,
        args: &[Reg],
        dest: Option<Reg>,
        span: Span,
        current: &Current<'ctx, 'm>,
    ) -> Result<Option<BasicValueEnum<'ctx>>> {
        let builder = &self.builder;
        let i64_type = self.context.i64_type();

        let slots_type = i64_type.array_type(2 * args.len().max(1) as u32);
        let mut slots = slots_type.const_zero();
        for (position, arg) in args.iter().enumerate() {
            let (tag, bits) = self.to_slot(*arg, current)?;
            let index = 2 * position as u32;
            slots = builder
                .build_insert_value(slots, tag, index, "")?
                .into_array_value();
            slots = builder
                .build_insert_value(slots, bits, index + 1, "")?
                .into_array_value();
        }
        let slots_ptr = self.stack_slot(slots_type.into(), current)?;
        builder.build_store(slots_ptr, slots)?;

        let result_type = i64_type.array_type(2);
        let result_ptr = self.stack_slot(result_type.into(), current)?;
        let (location_ptr, location_len) = self.string(&(self.locate)(span))?;
        builder.build_call(
            self.runtime_builtin,
            &[
                i64_type.const_int(self.builtins[id], false).into(),
                slots_ptr.into(),
                i64_type.const_int(args.len() as u64, false).into(),
                result_ptr.into(),
                location_ptr.into(),
                location_len.into(),
            ],
            "",
        )?;

        let Some(dest) = dest else {
            return Ok(None);
        };
        let result = builder
            .build_load(result_type, result_ptr, "")?
            .into_array_value();
        let bits = builder.build_extract_value(result, 1, "")?.into_int_value();
        Ok(Some(self.value_from_bits(bits, current.ty(dest))?))
    }

    /// The tag and bits of `reg`, as `kmpl_rt` expects them in a `Slot`.
    fn to_slot(
        &self,
        reg: Reg,
        current: &Current<'ctx, 'm>,
    ) -> Result<(IntValue<'ctx>, IntValue<'ctx>)> {
        let i64_type = self.context.i64_type();
        let value = current.value(reg);
        let (tag, bits) = match current.ty(reg) {
            Ty::Prim(Type::Int) => (abi::TAG_INT, value.into_int_value()),
            Ty::Prim(Type::Real) => {
                let bits = self.builder.build_bit_cast(value, i64_type, "")?;
                (abi::TAG_REAL, bits.into_int_value())
            }
            Ty::Prim(Type::Bool) => {
                let bits = self
                    .builder
                    .build_int_z_extend(value.into_int_value(), i64_type, "")?;
                (abi::TAG_BOOL, bits)
            }
            other => unreachable!("`supported` rejected {other:?}"),
        };
        Ok((i64_type.const_int(tag, false), bits))
    }

    fn value_from_bits(&self, bits: IntValue<'ctx>, ty: &Ty) -> Result<BasicValueEnum<'ctx>> {
        let value: BasicValueEnum = match ty {
            Ty::Prim(Type::Int) => bits.into(),
            Ty::Prim(Type::Real) => {
                self.builder
                    .build_bit_cast(bits, self.context.f64_type(), "")?
            }
            Ty::Prim(Type::Bool) => self
                .builder
                .build_int_truncate(bits, self.context.bool_type(), "")?
                .into(),
            other => unreachable!("`supported` rejected {other:?}"),
        };
        Ok(value)
    }

    /// Stack memory in the first block: an `alloca` inside a loop would take
    /// more stack on every iteration.
    fn stack_slot(
        &self,
        ty: BasicTypeEnum<'ctx>,
        current: &Current<'ctx, 'm>,
    ) -> Result<PointerValue<'ctx>> {
        let builder = self.context.create_builder();
        let jump = current
            .start
            .get_terminator()
            .expect("the first block already jumps onward");
        builder.position_before(&jump);
        builder.build_alloca(ty, "")
    }

    /// A constant string, as its address and length. Not zero-terminated.
    fn string(&self, text: &str) -> Result<(PointerValue<'ctx>, IntValue<'ctx>)> {
        let length = self.context.i64_type().const_int(text.len() as u64, false);
        if let Some(pointer) = self.strings.borrow().get(text) {
            return Ok((*pointer, length));
        }

        let bytes = self.context.const_string(text.as_bytes(), false);
        let global = self.llvm.add_global(bytes.get_type(), None, "");
        global.set_linkage(Linkage::Private);
        global.set_constant(true);
        global.set_initializer(&bytes);
        let pointer = global.as_pointer_value();
        self.strings.borrow_mut().insert(text.to_owned(), pointer);
        Ok((pointer, length))
    }
}

/// Reachable blocks, so that each comes after every block that dominates it.
fn reverse_postorder(function: &Function) -> Vec<usize> {
    let mut seen = vec![false; function.blocks.len()];
    let mut postorder = Vec::with_capacity(function.blocks.len());
    // A block and how many of its successors were already visited.
    let mut stack = vec![(0, 0)];
    seen[0] = true;

    while let Some((block, next)) = stack.last_mut() {
        let targets = function.blocks[*block].term.targets();
        match targets.get(*next) {
            Some(target) => {
                *next += 1;
                let successor = target.block.0 as usize;
                if !seen[successor] {
                    seen[successor] = true;
                    stack.push((successor, 0));
                }
            }
            None => {
                postorder.push(*block);
                stack.pop();
            }
        }
    }

    postorder.reverse();
    postorder
}
