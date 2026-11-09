//! Translates IR functions to LLVM functions. Both are SSA, and a block's
//! parameters holding numbers become phi nodes.

use std::cell::RefCell;
use std::collections::HashMap;

use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::{Builder, BuilderError};
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, PointerType};
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PhiValue, PointerValue};
use inkwell::{AddressSpace, IntPredicate};
use kmpl_ast::{Span, Ty, Type, UnOp};
use kmpl_builtins::{BUILTINS, abi, ops};
use kmpl_ir::{Constant, Function, Inst, Op, Reg, Step, Target, Terminator};

use crate::vector::vector_size;

/// The symbol `kmpl_rt` calls to start the program.
const ENTRY: &str = "kmpl_program";

pub(crate) type Result<T> = std::result::Result<T, BuilderError>;

/// The functions of `kmpl_rt`.
pub(crate) struct Runtime<'ctx> {
    pub init: FunctionValue<'ctx>,
    pub error: FunctionValue<'ctx>,
    pub builtin: FunctionValue<'ctx>,
    pub op: FunctionValue<'ctx>,
    pub clone: FunctionValue<'ctx>,
    pub drop: FunctionValue<'ctx>,
    pub box_vector: FunctionValue<'ctx>,
    pub unbox_vector: FunctionValue<'ctx>,
}

pub struct Translator<'ctx, 'm> {
    pub(crate) context: &'ctx Context,
    pub(crate) llvm: Module<'ctx>,
    pub(crate) builder: Builder<'ctx>,
    pub(crate) ir: &'m kmpl_ir::Module,
    pub(crate) locate: &'m dyn Fn(Span) -> String,
    functions: Vec<FunctionValue<'ctx>>,
    globals: Vec<Option<PointerValue<'ctx>>>,
    /// How many calls are active, to stop runaway recursion.
    depth: PointerValue<'ctx>,
    pub(crate) runtime: Runtime<'ctx>,
    /// For each entry of `Module::builtins`, its position in `BUILTINS`.
    pub(crate) builtins: Vec<u64>,
    strings: RefCell<HashMap<String, PointerValue<'ctx>>>,
}

/// What is known about the LLVM function being generated.
pub(crate) struct Current<'ctx, 'm> {
    ir: &'m Function,
    pub llvm: FunctionValue<'ctx>,
    blocks: Vec<BasicBlock<'ctx>>,
    /// For each block, a phi for each parameter that holds a number.
    phis: Vec<Vec<Option<PhiValue<'ctx>>>>,
    values: Vec<Option<BasicValueEnum<'ctx>>>,
    /// The stack slot of every register that holds a boxed value.
    slots: Vec<Option<PointerValue<'ctx>>>,
    /// Where arguments for the runtime are written.
    pub arguments: PointerValue<'ctx>,
    /// Where the runtime writes a result.
    pub result: PointerValue<'ctx>,
    /// Every jump, as its target and the block it leaves from.
    edges: Vec<(&'m Target, BasicBlock<'ctx>)>,
}

impl<'ctx> Current<'ctx, '_> {
    pub fn value(&self, reg: Reg) -> BasicValueEnum<'ctx> {
        self.values[reg.0 as usize].expect("a register is defined before it is used")
    }

    pub fn int(&self, reg: Reg) -> IntValue<'ctx> {
        self.value(reg).into_int_value()
    }

    pub fn ty(&self, reg: Reg) -> &Ty {
        &self.ir.types[reg.0 as usize]
    }

    pub fn slot(&self, reg: Reg) -> Option<PointerValue<'ctx>> {
        self.slots[reg.0 as usize]
    }

    /// Which number `reg` holds, as the tag of an `abi::Slot`.
    pub fn type_kind(&self, reg: Reg) -> u64 {
        match self.ty(reg) {
            Ty::Prim(Type::Int) => abi::TAG_INT,
            Ty::Prim(Type::Real) => abi::TAG_REAL,
            Ty::Prim(Type::Bool) => abi::TAG_BOOL,
            other => unreachable!("{other:?} is not a plain number"),
        }
    }

    fn is_number(&self, reg: Reg) -> bool {
        self.slot(reg).is_none()
    }
}

/// Does a value of this type live in machine registers?
fn is_unboxed(ty: &Ty) -> bool {
    matches!(ty, Ty::Prim(Type::Int | Type::Real | Type::Bool)) || vector_size(ty).is_some()
}

impl<'ctx, 'm> Translator<'ctx, 'm> {
    pub fn new(
        context: &'ctx Context,
        ir: &'m kmpl_ir::Module,
        locate: &'m dyn Fn(Span) -> String,
    ) -> Self {
        let llvm = context.create_module("kmpl");
        let i64_type = context.i64_type();

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
            runtime: declare_runtime(context, &llvm),
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

    pub(crate) fn pointer_type(&self) -> PointerType<'ctx> {
        self.context.ptr_type(AddressSpace::default())
    }

    fn llvm_type(&self, ty: &Ty) -> BasicTypeEnum<'ctx> {
        match ty {
            Ty::Prim(Type::Int) => self.context.i64_type().into(),
            Ty::Prim(Type::Real) => self.context.f64_type().into(),
            Ty::Prim(Type::Bool) => self.context.bool_type().into(),
            other => match vector_size(other) {
                Some(size) => self.vector_type(size).into(),
                None => self.pointer_type().into(),
            },
        }
    }

    /// The IR has no list of global types, so they come from their uses.
    fn declare_globals(&mut self) {
        let mut types: Vec<Option<&Ty>> = vec![None; self.ir.globals];
        for function in &self.ir.functions {
            for inst in function.blocks.iter().flat_map(|b| &b.insts) {
                match (&inst.op, inst.dest) {
                    (Op::SetGlobal(id, reg), _) => {
                        types[*id] = Some(&function.types[reg.0 as usize]);
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

    /// Tells the runtime the struct layouts, runs the constants, then `main`.
    fn entry(&self) -> Result<()> {
        let signature = self.context.void_type().fn_type(&[], false);
        let entry = self.llvm.add_function(ENTRY, signature, None);
        self.builder
            .position_at_end(self.context.append_basic_block(entry, "start"));

        let (layouts, layouts_length) = self.string(&struct_layouts(self.ir))?;
        self.builder.build_call(
            self.runtime.init,
            &[layouts.into(), layouts_length.into()],
            "",
        )?;
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
        let i64_type = self.context.i64_type();
        let start = self.context.append_basic_block(llvm, "start");
        let blocks: Vec<BasicBlock> = ir
            .blocks
            .iter()
            .map(|_| self.context.append_basic_block(llvm, ""))
            .collect();

        // Stack memory goes in the first block: an `alloca` inside a loop
        // would take more stack on every iteration.
        self.builder.position_at_end(start);
        let mut slots = vec![None; ir.types.len()];
        for (reg, ty) in ir.types.iter().enumerate() {
            if !is_unboxed(ty) {
                let slot = self.builder.build_alloca(self.pointer_type(), "")?;
                self.builder
                    .build_store(slot, self.pointer_type().const_null())?;
                slots[reg] = Some(slot);
            }
        }
        let arguments_type = i64_type.array_type(2 * buffer_slots(ir));
        let arguments = self.builder.build_alloca(arguments_type, "")?;
        // Room for the largest vector.
        let result = self.builder.build_alloca(i64_type.array_type(4), "")?;

        let mut current = Current {
            ir,
            llvm,
            blocks,
            phis: vec![Vec::new(); ir.blocks.len()],
            values: vec![None; ir.types.len()],
            slots,
            arguments,
            result,
            edges: Vec::new(),
        };

        // Only blocks that can run, each after the blocks that dominate it,
        // so a register is always defined before it is read.
        let order = reverse_postorder(ir);
        for &block in &order {
            self.builder.position_at_end(current.blocks[block]);
            for param in &ir.blocks[block].params {
                let phi = match current.is_number(*param) {
                    true => {
                        let ty = self.llvm_type(current.ty(*param));
                        let phi = self.builder.build_phi(ty, "")?;
                        current.values[param.0 as usize] = Some(phi.as_basic_value());
                        Some(phi)
                    }
                    false => None,
                };
                current.phis[block].push(phi);
            }
        }

        self.builder.position_at_end(start);
        for (position, param) in ir.params.iter().enumerate() {
            let argument = llvm
                .get_nth_param(position as u32)
                .expect("declared with these parameters");
            if let Some(slot) = current.slot(*param) {
                self.builder.build_store(slot, argument)?;
                continue;
            }
            let phi = ir.blocks[0].params.iter().position(|p| p == param);
            match phi.and_then(|phi| current.phis[0][phi]) {
                Some(phi) => phi.add_incoming(&[(&argument, start)]),
                None => current.values[param.0 as usize] = Some(argument),
            }
        }
        self.builder.build_unconditional_branch(current.blocks[0])?;

        for &block in &order {
            self.builder.position_at_end(current.blocks[block]);
            for inst in &ir.blocks[block].insts {
                self.inst(inst, &mut current)?;
            }
            let block = &ir.blocks[block];
            self.terminator(&block.term, &block.term_frees, &mut current)?;
        }

        // Last, because a jump back up a loop passes values defined later.
        let edges = std::mem::take(&mut current.edges);
        for (target, from) in edges {
            let phis = &current.phis[target.block.0 as usize];
            for (phi, arg) in phis.iter().zip(&target.args) {
                if let Some(phi) = phi {
                    phi.add_incoming(&[(&current.value(*arg), from)]);
                }
            }
        }
        Ok(())
    }

    fn terminator(
        &self,
        term: &'m Terminator,
        frees: &[Reg],
        current: &mut Current<'ctx, 'm>,
    ) -> Result<()> {
        let builder = &self.builder;
        match term {
            Terminator::Jump(target) => self.jump(target, frees, current)?,
            Terminator::Branch {
                cond,
                then,
                otherwise,
            } => {
                let condition = current.int(*cond);
                let simple = then.block != otherwise.block
                    && !self.passes_boxed(then, frees, current)
                    && !self.passes_boxed(otherwise, frees, current);
                if simple {
                    let here = builder.get_insert_block().expect("positioned");
                    builder.build_conditional_branch(
                        condition,
                        current.blocks[then.block.0 as usize],
                        current.blocks[otherwise.block.0 as usize],
                    )?;
                    current.edges.push((then, here));
                    current.edges.push((otherwise, here));
                    return Ok(());
                }

                // Each way needs its own code. A phi could not tell two
                // edges from one block apart, either.
                let then_edge = self.context.append_basic_block(current.llvm, "");
                let else_edge = self.context.append_basic_block(current.llvm, "");
                builder.build_conditional_branch(condition, then_edge, else_edge)?;
                builder.position_at_end(then_edge);
                self.jump(then, frees, current)?;
                builder.position_at_end(else_edge);
                self.jump(otherwise, frees, current)?;
            }
            Terminator::Return(value) => {
                let result = match value {
                    Some(reg) => Some(match current.slot(*reg) {
                        Some(slot) => BasicValueEnum::from(self.take(slot)?),
                        None => current.value(*reg),
                    }),
                    None => None,
                };
                for slot in current.slots.iter().flatten() {
                    self.release(*slot, current)?;
                }
                builder.build_return(result.as_ref().map(|v| v as _))?;
            }
            Terminator::Unreachable => {
                builder.build_unreachable()?;
            }
        }
        Ok(())
    }

    /// Does this jump do anything besides passing numbers?
    fn passes_boxed(&self, target: &Target, frees: &[Reg], current: &Current<'ctx, 'm>) -> bool {
        frees.iter().any(|reg| !current.is_number(*reg))
            || target.args.iter().any(|arg| !current.is_number(*arg))
    }

    /// Reads every argument first, because a jump may swap two parameters.
    fn jump(
        &self,
        target: &'m Target,
        frees: &[Reg],
        current: &mut Current<'ctx, 'm>,
    ) -> Result<()> {
        let destination = &current.ir.blocks[target.block.0 as usize];
        let mut passed = Vec::new();
        for (param, arg) in destination.params.iter().zip(&target.args) {
            let Some(slot) = current.slot(*arg) else {
                continue;
            };
            let once = target.args.iter().filter(|a| *a == arg).count() == 1;
            let value = match frees.contains(arg) && once {
                true => self.take(slot)?,
                false => self.clone_value(self.load_pointer(slot)?)?,
            };
            passed.push((*param, value));
        }
        self.release_all(frees, current)?;
        for (param, value) in passed {
            let slot = current.slot(param).expect("the argument is boxed too");
            self.release(slot, current)?;
            self.builder.build_store(slot, value)?;
        }

        let from = self.builder.get_insert_block().expect("positioned");
        self.builder
            .build_unconditional_branch(current.blocks[target.block.0 as usize])?;
        current.edges.push((target, from));
        Ok(())
    }

    fn release_all(&self, regs: &[Reg], current: &Current<'ctx, 'm>) -> Result<()> {
        for reg in regs {
            if let Some(slot) = current.slot(*reg) {
                self.release(slot, current)?;
            }
        }
        Ok(())
    }

    fn inst(&self, inst: &Inst, current: &mut Current<'ctx, 'm>) -> Result<()> {
        let result = self.operation(inst, current)?;
        if let (Some(dest), Some(value)) = (inst.dest, result) {
            match current.slot(dest) {
                Some(slot) => {
                    self.release(slot, current)?;
                    self.builder.build_store(slot, value)?;
                }
                None => current.values[dest.0 as usize] = Some(value),
            }
        }
        // A call released these before it started.
        if !matches!(inst.op, Op::Call { .. }) {
            self.release_all(&inst.frees, current)?;
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
            Op::Const(Constant::Str(text)) => return self.boxed_string(text, inst, current),
            Op::Const(Constant::Complex { re, im }) => self.vector_constant(&[*re, *im])?,
            Op::Const(constant) => self.constant(constant),
            Op::Swizzle { base, mask } => self.swizzle(*base, mask, current)?,
            Op::Unary { op: UnOp::Not, a } if current.is_number(*a) => {
                self.builder.build_not(current.int(*a), "")?.into()
            }
            Op::Unary { op, a } => match current.ty(*a) {
                ty if vector_size(ty).is_some() => self.vector_unary(*op, *a, current)?,
                _ if current.is_number(*a) => self.number_neg(*a, span, current)?,
                _ => return self.boxed_unary(*op, *a, inst, current),
            },
            Op::Binary { op, a, b } => {
                if !current.is_number(*a) || !current.is_number(*b) {
                    return self.boxed_binary(abi::bin_op_code(*op), *a, *b, inst, current);
                }
                let in_vector = |reg: &Reg| vector_size(current.ty(*reg)).is_some();
                match in_vector(a) || in_vector(b) {
                    true => self.vector_binary(*op, *a, *b, current)?,
                    false => self.number_binary(*op, *a, *b, span, current)?,
                }
            }
            Op::Call { func, args } => return self.call(func.0 as usize, args, inst, current),
            Op::Builtin { id, args } => return self.builtin(*id, args, inst, current),
            Op::GetGlobal(id) => {
                let dest = inst.dest.expect("reading a global has a result");
                let global = self.globals[*id].expect("this use gave the global a type");
                let ty = self.llvm_type(current.ty(dest));
                let value = self.builder.build_load(ty, global, "")?;
                match current.slot(dest) {
                    Some(_) => self.clone_value(value.into_pointer_value())?.into(),
                    None => value,
                }
            }
            Op::SetGlobal(id, reg) => {
                let global = self.globals[*id].expect("this use gave the global a type");
                let value: BasicValueEnum = match current.slot(*reg) {
                    Some(slot) => self.clone_value(self.load_pointer(slot)?)?.into(),
                    None => current.value(*reg),
                };
                self.builder.build_store(global, value)?;
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
            _ => return self.boxed_operation(inst, current),
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
            other => unreachable!("{other:?} is boxed"),
        }
    }

    /// Stops the program with `message` if `condition` holds. Afterwards the
    /// builder is in a new block, the one where it did not hold.
    pub(crate) fn fail_if(
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
        let (location_ptr, location_len) = self.location(span)?;
        self.builder.build_call(
            self.runtime.error,
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
        inst: &Inst,
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
        self.fail_if(too_deep, &ops::stack_overflow_message(), inst.span, current)?;
        builder.build_store(self.depth, deeper)?;

        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            let value: BasicValueEnum = match current.slot(*arg) {
                Some(slot) => {
                    let once = args.iter().filter(|a| *a == arg).count() == 1;
                    match inst.frees.contains(arg) && once {
                        true => self.take(slot)?.into(),
                        false => self.clone_value(self.load_pointer(slot)?)?.into(),
                    }
                }
                None => current.value(*arg),
            };
            values.push(value.into());
        }
        // Before the call, so the callee can own these values.
        self.release_all(&inst.frees, current)?;

        let result = builder.build_call(self.functions[function], &values, "")?;
        builder.build_store(self.depth, depth)?;
        Ok(result.try_as_basic_value().basic())
    }

    pub(crate) fn location(&self, span: Span) -> Result<(PointerValue<'ctx>, IntValue<'ctx>)> {
        self.string(&(self.locate)(span))
    }

    /// A constant string, as its address and length. Not zero-terminated.
    pub(crate) fn string(&self, text: &str) -> Result<(PointerValue<'ctx>, IntValue<'ctx>)> {
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

fn declare_runtime<'ctx>(context: &'ctx Context, llvm: &Module<'ctx>) -> Runtime<'ctx> {
    let void = context.void_type();
    let ptr_type = context.ptr_type(AddressSpace::default());
    let ptr: BasicMetadataTypeEnum = ptr_type.into();
    let int: BasicMetadataTypeEnum = context.i64_type().into();
    let real: BasicMetadataTypeEnum = context.f64_type().into();

    let error = llvm.add_function(
        "kmpl_rt_error",
        void.fn_type(&[ptr, int, ptr, int], false),
        None,
    );
    for attribute in ["noreturn", "cold"] {
        let kind = Attribute::get_named_enum_kind_id(attribute);
        error.add_attribute(
            AttributeLoc::Function,
            context.create_enum_attribute(kind, 0),
        );
    }

    Runtime {
        init: llvm.add_function("kmpl_rt_init", void.fn_type(&[ptr, int], false), None),
        error,
        builtin: llvm.add_function(
            "kmpl_rt_builtin",
            void.fn_type(&[int, ptr, int, ptr, ptr, int], false),
            None,
        ),
        op: llvm.add_function(
            "kmpl_rt_op",
            void.fn_type(&[int, int, ptr, int, ptr, ptr, int], false),
            None,
        ),
        clone: llvm.add_function("kmpl_rt_clone", ptr_type.fn_type(&[ptr], false), None),
        drop: llvm.add_function("kmpl_rt_drop", void.fn_type(&[ptr], false), None),
        box_vector: llvm.add_function(
            "kmpl_rt_box_vector",
            ptr_type.fn_type(&[int, real, real, real, real], false),
            None,
        ),
        unbox_vector: llvm.add_function(
            "kmpl_rt_unbox_vector",
            void.fn_type(&[ptr, ptr], false),
            None,
        ),
    }
}

/// One line per struct: its name, then the names of its fields.
fn struct_layouts(module: &kmpl_ir::Module) -> String {
    let lines: Vec<String> = module
        .structs
        .iter()
        .map(|s| {
            let mut words = vec![s.name.as_str()];
            words.extend(s.fields.iter().map(String::as_str));
            words.join(" ")
        })
        .collect();
    lines.join("\n")
}

/// How many `abi::Slot`s the largest runtime call of this function passes.
fn buffer_slots(function: &Function) -> u32 {
    let needed = |inst: &Inst| {
        let mut uses = 0;
        inst.op.for_each_use(|_| uses += 1);
        // An index also passes where a bad index is reported.
        let path: &[Step] = match &inst.op {
            Op::PathGet { path, .. } | Op::PathSet { path, .. } => path,
            _ => &[],
        };
        uses + path.len() + 2
    };
    function
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .map(needed)
        .max()
        .unwrap_or(1) as u32
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
